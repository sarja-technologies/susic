use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

const MIGRATIONS: &[&str] = &[
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
     INSERT INTO meta VALUES ('rev', '0');
     CREATE TABLE tracks (
        id INTEGER PRIMARY KEY,
        rel_path TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL,
        artist TEXT NOT NULL DEFAULT '',
        album TEXT NOT NULL DEFAULT '',
        album_artist TEXT NOT NULL DEFAULT '',
        duration_ms INTEGER NOT NULL DEFAULT 0,
        track_no INTEGER, disc_no INTEGER, year INTEGER,
        format TEXT NOT NULL DEFAULT '',
        size_bytes INTEGER NOT NULL DEFAULT 0,
        mtime INTEGER NOT NULL DEFAULT 0,
        hash TEXT NOT NULL DEFAULT '',
        artwork TEXT,
        lyrics TEXT,
        favourite INTEGER NOT NULL DEFAULT 0,
        missing INTEGER NOT NULL DEFAULT 0,
        added_at INTEGER NOT NULL,
        rev INTEGER NOT NULL
     );
     CREATE INDEX idx_tracks_hash ON tracks(hash);
     CREATE INDEX idx_tracks_rev ON tracks(rev);
     CREATE TABLE devices (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        created_at INTEGER NOT NULL,
        last_seen INTEGER NOT NULL DEFAULT 0,
        revoked INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE pair_codes (code_hash TEXT PRIMARY KEY, expires_at INTEGER NOT NULL);
     CREATE TABLE playlists (
        uid TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        deleted INTEGER NOT NULL DEFAULT 0,
        rev INTEGER NOT NULL
     );
     CREATE TABLE playlist_items (
        uid TEXT NOT NULL REFERENCES playlists(uid) ON DELETE CASCADE,
        pos INTEGER NOT NULL,
        track_id INTEGER NOT NULL,
        PRIMARY KEY (uid, pos)
     );",
];

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Msg(String),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackRow {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_ms: i64,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub year: Option<i64>,
    pub format: String,
    pub size_bytes: i64,
    pub hash: String,
    pub has_artwork: bool,
    pub has_lyrics: bool,
    pub favourite: bool,
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistRow {
    pub uid: String,
    pub name: String,
    pub updated_at: i64,
    pub deleted: bool,
    pub track_ids: Vec<i64>,
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceRow {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub last_seen: i64,
    pub revoked: bool,
}

/// Row being written by the scanner.
pub struct ScanTrack {
    pub rel_path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_ms: i64,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub year: Option<i64>,
    pub format: String,
    pub size_bytes: i64,
    pub mtime: i64,
    pub hash: String,
    pub artwork: Option<String>,
    pub lyrics: Option<String>,
}

/// (rel_path, id, size_bytes, mtime, missing)
pub type PathEntry = (String, i64, i64, i64, bool);

pub struct Db {
    conn: Mutex<Connection>,
}

pub fn hash_secret(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex().to_string()
}

const TRACK_COLS: &str =
    "id, title, artist, album, album_artist, duration_ms, track_no, disc_no, year, \
     format, size_bytes, hash, artwork IS NOT NULL, lyrics IS NOT NULL, favourite, rev";

fn track_from_row(r: &rusqlite::Row) -> rusqlite::Result<TrackRow> {
    Ok(TrackRow {
        id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        album: r.get(3)?,
        album_artist: r.get(4)?,
        duration_ms: r.get(5)?,
        track_no: r.get(6)?,
        disc_no: r.get(7)?,
        year: r.get(8)?,
        format: r.get(9)?,
        size_bytes: r.get(10)?,
        hash: r.get(11)?,
        has_artwork: r.get(12)?,
        has_lyrics: r.get(13)?,
        favourite: r.get::<_, i64>(14)? != 0,
        rev: r.get(15)?,
    })
}

fn bump_rev(conn: &Connection) -> rusqlite::Result<i64> {
    conn.execute(
        "UPDATE meta SET value = CAST(value AS INTEGER) + 1 WHERE key = 'rev'",
        [],
    )?;
    conn.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key='rev'",
        [],
        |r| r.get(0),
    )
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Self::init(conn)
    }

    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        let current: usize =
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn c(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn revision(&self) -> Result<i64> {
        Ok(self.c().query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='rev'",
            [],
            |r| r.get(0),
        )?)
    }

    // ---- library ----
    /// Every known row as (rel_path, id, size, mtime, missing).
    pub fn path_index(&self) -> Result<Vec<PathEntry>> {
        let c = self.c();
        let mut st = c.prepare("SELECT rel_path, id, size_bytes, mtime, missing FROM tracks")?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn live_count(&self) -> Result<i64> {
        Ok(self
            .c()
            .query_row("SELECT COUNT(*) FROM tracks WHERE missing=0", [], |r| {
                r.get(0)
            })?)
    }

    /// Insert or update the row for `t.rel_path`, preserving id and favourite.
    pub fn upsert_scanned(&self, t: &ScanTrack) -> Result<i64> {
        let c = self.c();
        let tx = c.unchecked_transaction()?;
        let rev = bump_rev(&tx)?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM tracks WHERE rel_path=?1",
                [&t.rel_path],
                |r| r.get(0),
            )
            .optional()?;
        let id = if let Some(id) = existing {
            tx.execute(
                "UPDATE tracks SET title=?2, artist=?3, album=?4, album_artist=?5, duration_ms=?6,
                    track_no=?7, disc_no=?8, year=?9, format=?10, size_bytes=?11, mtime=?12, hash=?13,
                    artwork=?14, lyrics=?15, missing=0, rev=?16 WHERE id=?1",
                params![id, t.title, t.artist, t.album, t.album_artist, t.duration_ms, t.track_no,
                    t.disc_no, t.year, t.format, t.size_bytes, t.mtime, t.hash, t.artwork, t.lyrics, rev],
            )?;
            id
        } else {
            tx.execute(
                "INSERT INTO tracks (rel_path, title, artist, album, album_artist, duration_ms, track_no,
                    disc_no, year, format, size_bytes, mtime, hash, artwork, lyrics, added_at, rev)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![t.rel_path, t.title, t.artist, t.album, t.album_artist, t.duration_ms, t.track_no,
                    t.disc_no, t.year, t.format, t.size_bytes, t.mtime, t.hash, t.artwork, t.lyrics, now(), rev],
            )?;
            tx.last_insert_rowid()
        };
        tx.commit()?;
        Ok(id)
    }

    /// A file that vanished from `old` and reappeared at `new_path` keeps its id
    /// (and therefore favourites and playlist membership).
    pub fn move_track(&self, id: i64, new_path: &str, mtime: i64) -> Result<()> {
        let c = self.c();
        let tx = c.unchecked_transaction()?;
        let rev = bump_rev(&tx)?;
        tx.execute(
            "UPDATE tracks SET rel_path=?2, mtime=?3, missing=0, rev=?4 WHERE id=?1",
            params![id, new_path, mtime, rev],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_missing(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let c = self.c();
        let tx = c.unchecked_transaction()?;
        let rev = bump_rev(&tx)?;
        for id in ids {
            tx.execute(
                "UPDATE tracks SET missing=1, rev=?2 WHERE id=?1",
                params![id, rev],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Find a track no longer present on disk with the same fingerprint.
    pub fn find_by_hash(&self, hash: &str, among: &[i64]) -> Result<Option<i64>> {
        let c = self.c();
        let mut st = c.prepare("SELECT id FROM tracks WHERE hash=?1")?;
        let ids: Vec<i64> = st
            .query_map([hash], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids.into_iter().find(|i| among.contains(i)))
    }

    pub fn rel_path(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .c()
            .query_row(
                "SELECT rel_path FROM tracks WHERE id=?1 AND missing=0",
                [id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn artwork(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .c()
            .query_row(
                "SELECT artwork FROM tracks WHERE id=?1 AND missing=0",
                [id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn lyrics(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .c()
            .query_row(
                "SELECT lyrics FROM tracks WHERE id=?1 AND missing=0",
                [id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Tracks changed after `since` plus ids removed after `since`.
    pub fn changes(&self, since: i64) -> Result<(Vec<TrackRow>, Vec<i64>)> {
        let c = self.c();
        let sql =
            format!("SELECT {TRACK_COLS} FROM tracks WHERE missing=0 AND rev > ?1 ORDER BY id");
        let mut st = c.prepare(&sql)?;
        let tracks = st
            .query_map([since], track_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut st = c.prepare("SELECT id FROM tracks WHERE missing=1 AND rev > ?1")?;
        let removed = st
            .query_map([since], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((tracks, removed))
    }

    pub fn set_favourite(&self, id: i64, fav: bool) -> Result<bool> {
        let c = self.c();
        let tx = c.unchecked_transaction()?;
        let rev = bump_rev(&tx)?;
        let n = tx.execute(
            "UPDATE tracks SET favourite=?2, rev=?3 WHERE id=?1 AND missing=0",
            params![id, fav as i64, rev],
        )?;
        tx.commit()?;
        Ok(n > 0)
    }

    pub fn purge_missing(&self) -> Result<usize> {
        Ok(self.c().execute("DELETE FROM tracks WHERE missing=1", [])?)
    }

    // ---- devices & pairing ----
    pub fn add_pair_code(&self, code_hash: &str, expires_at: i64) -> Result<()> {
        let c = self.c();
        c.execute("DELETE FROM pair_codes WHERE expires_at < ?1", [now()])?;
        c.execute(
            "INSERT OR REPLACE INTO pair_codes VALUES (?1, ?2)",
            params![code_hash, expires_at],
        )?;
        Ok(())
    }

    /// Atomically consume a valid pairing code. Returns whether it was valid.
    pub fn consume_pair_code(&self, code_hash: &str) -> Result<bool> {
        let n = self.c().execute(
            "DELETE FROM pair_codes WHERE code_hash=?1 AND expires_at >= ?2",
            params![code_hash, now()],
        )?;
        Ok(n > 0)
    }

    pub fn add_device(&self, name: &str, token_hash: &str) -> Result<i64> {
        let c = self.c();
        c.execute(
            "INSERT INTO devices (name, token_hash, created_at, last_seen) VALUES (?1, ?2, ?3, ?3)",
            params![name, token_hash, now()],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Returns the device id when `token_hash` belongs to a non-revoked device.
    pub fn authenticate(&self, token_hash: &str) -> Result<Option<i64>> {
        let c = self.c();
        let row: Option<(i64, i64)> = c
            .query_row(
                "SELECT id, last_seen FROM devices WHERE token_hash=?1 AND revoked=0",
                [token_hash],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, seen)) = row {
            let t = now();
            if t - seen > 60 {
                c.execute(
                    "UPDATE devices SET last_seen=?2 WHERE id=?1",
                    params![id, t],
                )?;
            }
            return Ok(Some(id));
        }
        Ok(None)
    }

    pub fn devices(&self) -> Result<Vec<DeviceRow>> {
        let c = self.c();
        let mut st =
            c.prepare("SELECT id, name, created_at, last_seen, revoked FROM devices ORDER BY id")?;
        let rows = st
            .query_map([], |r| {
                Ok(DeviceRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    last_seen: r.get(3)?,
                    revoked: r.get::<_, i64>(4)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn revoke_device(&self, id: i64) -> Result<bool> {
        Ok(self
            .c()
            .execute("UPDATE devices SET revoked=1 WHERE id=?1", [id])?
            > 0)
    }

    // ---- playlists ----
    pub fn playlists_since(&self, since: i64) -> Result<Vec<PlaylistRow>> {
        let c = self.c();
        let mut st = c.prepare(
            "SELECT uid, name, updated_at, deleted, rev FROM playlists WHERE rev > ?1 ORDER BY rev",
        )?;
        let heads = st
            .query_map([since], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)? != 0,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::new();
        for (uid, name, updated_at, deleted, rev) in heads {
            let mut st =
                c.prepare("SELECT track_id FROM playlist_items WHERE uid=?1 ORDER BY pos")?;
            let track_ids = st
                .query_map([&uid], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()?;
            out.push(PlaylistRow {
                uid,
                name,
                updated_at,
                deleted,
                track_ids,
                rev,
            });
        }
        Ok(out)
    }

    pub fn get_playlist(&self, uid: &str) -> Result<Option<PlaylistRow>> {
        let c = self.c();
        let head: Option<(String, i64, bool, i64)> = c
            .query_row(
                "SELECT name, updated_at, deleted, rev FROM playlists WHERE uid=?1",
                [uid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0, r.get(3)?)),
            )
            .optional()?;
        let Some((name, updated_at, deleted, rev)) = head else {
            return Ok(None);
        };
        let mut st = c.prepare("SELECT track_id FROM playlist_items WHERE uid=?1 ORDER BY pos")?;
        let track_ids = st
            .query_map([uid], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(Some(PlaylistRow {
            uid: uid.into(),
            name,
            updated_at,
            deleted,
            track_ids,
            rev,
        }))
    }

    /// Last-writer-wins by `updated_at`. Returns the stored row and whether the
    /// incoming version was applied.
    pub fn put_playlist(
        &self,
        uid: &str,
        name: &str,
        track_ids: &[i64],
        updated_at: i64,
        deleted: bool,
    ) -> Result<(PlaylistRow, bool)> {
        if let Some(cur) = self.get_playlist(uid)? {
            if cur.updated_at > updated_at {
                return Ok((cur, false));
            }
        }
        {
            let c = self.c();
            let tx = c.unchecked_transaction()?;
            let rev = bump_rev(&tx)?;
            tx.execute(
                "INSERT INTO playlists (uid, name, updated_at, deleted, rev) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(uid) DO UPDATE SET name=?2, updated_at=?3, deleted=?4, rev=?5",
                params![uid, name, updated_at, deleted as i64, rev],
            )?;
            tx.execute("DELETE FROM playlist_items WHERE uid=?1", [uid])?;
            if !deleted {
                for (i, t) in track_ids.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO playlist_items (uid, pos, track_id) VALUES (?1,?2,?3)",
                        params![uid, i as i64, t],
                    )?;
                }
            }
            tx.commit()?;
        }
        Ok((self.get_playlist(uid)?.expect("just written"), true))
    }

    pub fn stats(&self) -> Result<(i64, i64)> {
        let c = self.c();
        Ok(c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes),0) FROM tracks WHERE missing=0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }
}
