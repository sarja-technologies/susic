//! SQLite-backed music library: schema migrations, track/album/artist queries,
//! playlists, favourites, play history and settings.

mod migrations;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;
use susic_core::{AlbumSummary, ArtistSummary, NewTrack, Playlist, Track};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Msg(String),
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackQuery {
    pub search: Option<String>,
    /// title | artist | album | added | duration | played | plays
    pub sort: Option<String>,
    pub desc: Option<bool>,
    pub favourites_only: Option<bool>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackEdit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_no: Option<i64>,
    pub year: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryStats {
    pub tracks: i64,
    pub albums: i64,
    pub artists: i64,
    pub total_bytes: i64,
    pub total_duration_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    Inserted(i64),
    Updated(i64),
    /// Same audio content already exists at another path.
    Duplicate(i64),
}

pub struct Library {
    conn: Connection,
}

const TRACK_COLS: &str = "t.id, COALESCE(e.title,t.title), COALESCE(e.artist,t.artist), \
COALESCE(e.album,t.album), COALESCE(e.album_artist,t.album_artist), t.duration_ms, t.path, t.source, \
t.artwork, COALESCE(e.track_no,t.track_no), t.disc_no, COALESCE(e.year,t.year), t.added_at, \
t.play_count, t.last_played, t.favourite, (t.lyrics IS NOT NULL AND t.lyrics <> ''), t.hash, t.format, t.size_bytes";
const TRACK_FROM: &str = "FROM tracks t LEFT JOIN track_edits e ON e.track_id = t.id";

fn row_track(r: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        album: r.get(3)?,
        album_artist: r.get(4)?,
        duration_ms: r.get(5)?,
        path: r.get(6)?,
        source: r.get(7)?,
        artwork: r.get(8)?,
        track_no: r.get(9)?,
        disc_no: r.get(10)?,
        year: r.get(11)?,
        added_at: r.get(12)?,
        play_count: r.get(13)?,
        last_played: r.get(14)?,
        favourite: r.get::<_, i64>(15)? != 0,
        has_lyrics: r.get::<_, i64>(16)? != 0,
        hash: r.get(17)?,
        format: r.get(18)?,
        size_bytes: r.get(19)?,
    })
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    // ---- tracks -----------------------------------------------------------

    /// Insert a track, or update the existing row with the same path. A track
    /// with a different path but identical content hash is reported as a duplicate.
    pub fn upsert_track(&self, t: &NewTrack) -> Result<Upsert> {
        if let Some(id) = self
            .conn
            .query_row("SELECT id FROM tracks WHERE path=?1", [&t.path], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
        {
            self.conn.execute(
                "UPDATE tracks SET title=?2, artist=?3, album=?4, album_artist=?5, duration_ms=?6, \
                 artwork=COALESCE(?7, artwork), track_no=?8, disc_no=?9, year=?10, \
                 lyrics=COALESCE(?11, lyrics), hash=?12, format=?13, size_bytes=?14 WHERE id=?1",
                params![id, t.title, t.artist, t.album, t.album_artist, t.duration_ms, t.artwork,
                        t.track_no, t.disc_no, t.year, t.lyrics, t.hash, t.format, t.size_bytes],
            )?;
            return Ok(Upsert::Updated(id));
        }
        if !t.hash.is_empty() {
            if let Some(id) = self
                .conn
                .query_row("SELECT id FROM tracks WHERE hash=?1", [&t.hash], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()?
            {
                return Ok(Upsert::Duplicate(id));
            }
        }
        self.conn.execute(
            "INSERT INTO tracks (title, artist, album, album_artist, duration_ms, path, source, artwork, \
             track_no, disc_no, year, added_at, lyrics, hash, format, size_bytes) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![t.title, t.artist, t.album, t.album_artist, t.duration_ms, t.path, t.source,
                    t.artwork, t.track_no, t.disc_no, t.year, now(), t.lyrics, t.hash, t.format, t.size_bytes],
        )?;
        Ok(Upsert::Inserted(self.conn.last_insert_rowid()))
    }

    pub fn get_track(&self, id: i64) -> Result<Option<Track>> {
        let sql = format!("SELECT {TRACK_COLS} {TRACK_FROM} WHERE t.id=?1");
        Ok(self.conn.query_row(&sql, [id], row_track).optional()?)
    }

    /// Fetch tracks preserving the order of `ids`; unknown ids are skipped.
    pub fn get_tracks(&self, ids: &[i64]) -> Result<Vec<Track>> {
        let mut out = Vec::with_capacity(ids.len());
        let sql = format!("SELECT {TRACK_COLS} {TRACK_FROM} WHERE t.id=?1");
        let mut stmt = self.conn.prepare_cached(&sql)?;
        for id in ids {
            if let Some(t) = stmt.query_row([id], row_track).optional()? {
                out.push(t);
            }
        }
        Ok(out)
    }

    pub fn list_tracks(&self, q: &TrackQuery) -> Result<Vec<Track>> {
        let mut where_ = vec!["1=1".to_string()];
        let mut args: Vec<String> = vec![];
        if let Some(s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            for word in s.split_whitespace() {
                where_.push(
                    "(COALESCE(e.title,t.title) LIKE ? ESCAPE '\\' OR COALESCE(e.artist,t.artist) LIKE ? ESCAPE '\\' \
                     OR COALESCE(e.album,t.album) LIKE ? ESCAPE '\\')"
                        .into(),
                );
                let pat = format!(
                    "%{}%",
                    word.replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                );
                args.extend([pat.clone(), pat.clone(), pat]);
            }
        }
        if q.favourites_only == Some(true) {
            where_.push("t.favourite=1".into());
        }
        if let Some(a) = &q.album {
            where_.push("COALESCE(e.album,t.album)=?".into());
            args.push(a.clone());
        }
        if let Some(a) = &q.artist {
            where_.push(
                "(COALESCE(e.artist,t.artist)=? OR COALESCE(e.album_artist,t.album_artist)=?)"
                    .into(),
            );
            args.push(a.clone());
            args.push(a.clone());
        }
        let desc = q.desc.unwrap_or(false);
        let dir = if desc { "DESC" } else { "ASC" };
        let order = match q.sort.as_deref().unwrap_or("title") {
            "artist" => format!("COALESCE(e.artist,t.artist) COLLATE NOCASE {dir}, COALESCE(e.album,t.album) COLLATE NOCASE, COALESCE(e.track_no,t.track_no)"),
            "album" => format!("COALESCE(e.album,t.album) COLLATE NOCASE {dir}, t.disc_no, COALESCE(e.track_no,t.track_no)"),
            "added" => format!("t.added_at {dir}, t.id {dir}"),
            "duration" => format!("t.duration_ms {dir}"),
            "played" => format!("t.last_played IS NULL, t.last_played {dir}"),
            "plays" => format!("t.play_count {dir}"),
            "track" => "t.disc_no, COALESCE(e.track_no,t.track_no), COALESCE(e.title,t.title) COLLATE NOCASE".to_string(),
            _ => format!("COALESCE(e.title,t.title) COLLATE NOCASE {dir}"),
        };
        let sql = format!(
            "SELECT {TRACK_COLS} {TRACK_FROM} WHERE {} ORDER BY {order} LIMIT {} OFFSET {}",
            where_.join(" AND "),
            q.limit.unwrap_or(100_000).clamp(1, 1_000_000),
            q.offset.unwrap_or(0).max(0)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), row_track)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn recently_added(&self, limit: i64) -> Result<Vec<Track>> {
        self.list_tracks(&TrackQuery {
            sort: Some("added".into()),
            desc: Some(true),
            limit: Some(limit),
            ..Default::default()
        })
    }

    pub fn recently_played(&self, limit: i64) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {TRACK_COLS} {TRACK_FROM} WHERE t.last_played IS NOT NULL ORDER BY t.last_played DESC LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([limit], row_track)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn most_played(&self, limit: i64) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {TRACK_COLS} {TRACK_FROM} WHERE t.play_count > 0 ORDER BY t.play_count DESC, t.last_played DESC LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([limit], row_track)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Local rediscoveries: tracks that were played at least once but not in the
    /// last `days`, ordered by play count.
    pub fn rediscover(&self, days: i64, limit: i64) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {TRACK_COLS} {TRACK_FROM} WHERE t.play_count > 0 AND (t.last_played IS NULL OR t.last_played < ?1) \
             ORDER BY t.play_count DESC LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![now() - days * 86_400, limit], row_track)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_favourite(&self, id: i64, fav: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET favourite=?2 WHERE id=?1",
            params![id, fav as i64],
        )?;
        Ok(())
    }

    pub fn mark_played(&self, id: i64) -> Result<()> {
        let t = now();
        self.conn.execute(
            "UPDATE tracks SET play_count=play_count+1, last_played=?2 WHERE id=?1",
            params![id, t],
        )?;
        self.conn.execute(
            "INSERT INTO play_history (track_id, played_at) VALUES (?1, ?2)",
            params![id, t],
        )?;
        Ok(())
    }

    /// Store a user edit separately from the original file metadata.
    pub fn edit_track(&self, id: i64, e: &TrackEdit) -> Result<()> {
        self.conn.execute(
            "INSERT INTO track_edits (track_id, title, artist, album, album_artist, track_no, year) \
             VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(track_id) DO UPDATE SET \
             title=?2, artist=?3, album=?4, album_artist=?5, track_no=?6, year=?7",
            params![id, e.title, e.artist, e.album, e.album_artist, e.track_no, e.year],
        )?;
        Ok(())
    }

    pub fn clear_edit(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM track_edits WHERE track_id=?1", [id])?;
        Ok(())
    }

    /// Removes the library entry only. Never touches the audio file.
    pub fn remove_track(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM tracks WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn track_paths(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM tracks")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn known_path(&self, path: &str) -> Result<Option<(i64, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, size_bytes FROM tracks WHERE path=?1",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn track_by_hash(&self, hash: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT id FROM tracks WHERE hash=?1", [hash], |r| r.get(0))
            .optional()?)
    }

    // ---- lyrics -----------------------------------------------------------

    pub fn get_lyrics(&self, id: i64) -> Result<(Option<String>, i64)> {
        Ok(self
            .conn
            .query_row(
                "SELECT lyrics, lyrics_offset_ms FROM tracks WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, 0)))
    }

    pub fn set_lyrics(&self, id: i64, lyrics: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET lyrics=?2 WHERE id=?1",
            params![id, lyrics],
        )?;
        Ok(())
    }

    pub fn set_lyrics_offset(&self, id: i64, ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET lyrics_offset_ms=?2 WHERE id=?1",
            params![id, ms],
        )?;
        Ok(())
    }

    // ---- albums / artists -------------------------------------------------

    pub fn albums(&self, search: Option<&str>) -> Result<Vec<AlbumSummary>> {
        let like = format!("%{}%", search.unwrap_or("").trim());
        let sql = "SELECT album, MAX(aa), MAX(artwork), COUNT(*), SUM(duration_ms), MAX(year), SUM(favourite) FROM ( \
            SELECT COALESCE(e.album,t.album) AS album, COALESCE(e.artist,t.artist) AS artist, \
              COALESCE(NULLIF(COALESCE(e.album_artist,t.album_artist),''), COALESCE(e.artist,t.artist)) AS aa, \
              t.artwork AS artwork, t.duration_ms AS duration_ms, COALESCE(e.year,t.year) AS year, t.favourite AS favourite \
            FROM tracks t LEFT JOIN track_edits e ON e.track_id=t.id) \
            WHERE album <> '' AND (album LIKE ?1 OR artist LIKE ?1) \
            GROUP BY album, aa ORDER BY album COLLATE NOCASE";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([like], |r| {
            Ok(AlbumSummary {
                album: r.get(0)?,
                artist: r.get(1)?,
                artwork: r.get(2)?,
                track_count: r.get(3)?,
                duration_ms: r.get(4)?,
                year: r.get(5)?,
                favourite_count: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn artists(&self, search: Option<&str>) -> Result<Vec<ArtistSummary>> {
        let like = format!("%{}%", search.unwrap_or("").trim());
        let sql = "SELECT artist, MAX(artwork), COUNT(*), COUNT(DISTINCT album) FROM ( \
            SELECT COALESCE(e.artist,t.artist) AS artist, COALESCE(e.album,t.album) AS album, t.artwork AS artwork \
            FROM tracks t LEFT JOIN track_edits e ON e.track_id=t.id) \
            WHERE artist <> '' AND artist LIKE ?1 GROUP BY artist ORDER BY artist COLLATE NOCASE";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([like], |r| {
            Ok(ArtistSummary {
                artist: r.get(0)?,
                artwork: r.get(1)?,
                track_count: r.get(2)?,
                album_count: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn stats(&self) -> Result<LibraryStats> {
        let (tracks, bytes, dur): (i64, i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes),0), COALESCE(SUM(duration_ms),0) FROM tracks",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(LibraryStats {
            tracks,
            albums: self.albums(None)?.len() as i64,
            artists: self.artists(None)?.len() as i64,
            total_bytes: bytes,
            total_duration_ms: dur,
        })
    }

    // ---- playlists --------------------------------------------------------

    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Msg("Playlist name cannot be empty".into()));
        }
        let t = now();
        self.conn.execute(
            "INSERT INTO playlists (name, created_at, updated_at) VALUES (?1,?2,?2)",
            params![name, t],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE playlists SET name=?2, updated_at=?3 WHERE id=?1",
            params![id, name.trim(), now()],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM playlists WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let sql = "SELECT p.id, p.name, COUNT(pt.track_id), COALESCE(SUM(t.duration_ms),0), \
            (SELECT t2.artwork FROM playlist_tracks pt2 JOIN tracks t2 ON t2.id=pt2.track_id \
              WHERE pt2.playlist_id=p.id AND t2.artwork IS NOT NULL ORDER BY pt2.position LIMIT 1), p.updated_at \
            FROM playlists p LEFT JOIN playlist_tracks pt ON pt.playlist_id=p.id \
            LEFT JOIN tracks t ON t.id=pt.track_id GROUP BY p.id ORDER BY p.name COLLATE NOCASE";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(Playlist {
                id: r.get(0)?,
                name: r.get(1)?,
                track_count: r.get(2)?,
                duration_ms: r.get(3)?,
                artwork: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn playlist_track_ids(&self, id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT track_id FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn playlist_tracks(&self, id: i64) -> Result<Vec<Track>> {
        let ids = self.playlist_track_ids(id)?;
        self.get_tracks(&ids)
    }

    pub fn add_to_playlist(&self, id: i64, track_ids: &[i64]) -> Result<usize> {
        let start: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position)+1, 0) FROM playlist_tracks WHERE playlist_id=?1",
            [id],
            |r| r.get(0),
        )?;
        for (i, tid) in track_ids.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO playlist_tracks (playlist_id, position, track_id) VALUES (?1,?2,?3)",
                params![id, start + i as i64, tid],
            )?;
        }
        self.conn.execute(
            "UPDATE playlists SET updated_at=?2 WHERE id=?1",
            params![id, now()],
        )?;
        Ok(track_ids.len())
    }

    pub fn remove_from_playlist(&self, id: i64, position: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id=?1 AND position=?2",
            params![id, position],
        )?;
        self.renumber(id)
    }

    pub fn reorder_playlist(&self, id: i64, from: usize, to: usize) -> Result<()> {
        let mut ids = self.playlist_track_ids(id)?;
        if from >= ids.len() || to >= ids.len() {
            return Ok(());
        }
        let item = ids.remove(from);
        ids.insert(to, item);
        self.set_playlist_order(id, &ids)
    }

    pub fn set_playlist_order(&self, id: i64, ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id=?1", [id])?;
        for (i, tid) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, position, track_id) VALUES (?1,?2,?3)",
                params![id, i as i64, tid],
            )?;
        }
        tx.execute(
            "UPDATE playlists SET updated_at=?2 WHERE id=?1",
            params![id, now()],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn renumber(&self, id: i64) -> Result<()> {
        let ids = self.playlist_track_ids(id)?;
        self.set_playlist_order(id, &ids)
    }

    pub fn duplicate_playlist(&self, id: i64) -> Result<i64> {
        let name: String =
            self.conn
                .query_row("SELECT name FROM playlists WHERE id=?1", [id], |r| r.get(0))?;
        let new = self.create_playlist(&format!("{name} (copy)"))?;
        let ids = self.playlist_track_ids(id)?;
        self.add_to_playlist(new, &ids)?;
        Ok(new)
    }

    // ---- settings / state -------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- import jobs ------------------------------------------------------

    pub fn add_import_job(&self, url: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO import_jobs (url, status, progress, message, created_at) VALUES (?1,'queued',0,'',?2)",
            params![url, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_import_job(
        &self,
        id: i64,
        status: &str,
        progress: f64,
        message: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE import_jobs SET status=?2, progress=?3, message=?4 WHERE id=?1",
            params![id, status, progress, message],
        )?;
        Ok(())
    }

    pub fn import_jobs(&self) -> Result<Vec<ImportJob>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, status, progress, message, created_at FROM import_jobs ORDER BY id DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ImportJob {
                id: r.get(0)?,
                url: r.get(1)?,
                status: r.get(2)?,
                progress: r.get(3)?,
                message: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Jobs left in `running` state by a crash are marked interrupted.
    pub fn mark_interrupted_jobs(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE import_jobs SET status='interrupted', message='Interrupted — retry to resume' \
             WHERE status IN ('running','queued')",
            [],
        )?)
    }

    pub fn clear_finished_jobs(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM import_jobs WHERE status IN ('done','failed','cancelled','interrupted')",
            [],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportJob {
    pub id: i64,
    pub url: String,
    pub status: String,
    pub progress: f64,
    pub message: String,
    pub created_at: i64,
}

#[cfg(test)]
mod tests;
