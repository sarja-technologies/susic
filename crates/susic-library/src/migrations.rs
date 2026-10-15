use rusqlite::Connection;

/// Ordered, append-only migrations. `PRAGMA user_version` records how many have
/// been applied, so existing libraries are upgraded in place on launch.
const MIGRATIONS: &[&str] = &[
    // 1: core schema
    "CREATE TABLE tracks (
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        artist TEXT NOT NULL DEFAULT '',
        album TEXT NOT NULL DEFAULT '',
        album_artist TEXT NOT NULL DEFAULT '',
        duration_ms INTEGER NOT NULL DEFAULT 0,
        path TEXT NOT NULL UNIQUE,
        source TEXT NOT NULL DEFAULT 'local',
        artwork TEXT,
        track_no INTEGER,
        disc_no INTEGER,
        year INTEGER,
        added_at INTEGER NOT NULL,
        play_count INTEGER NOT NULL DEFAULT 0,
        last_played INTEGER,
        favourite INTEGER NOT NULL DEFAULT 0,
        lyrics TEXT,
        lyrics_offset_ms INTEGER NOT NULL DEFAULT 0,
        hash TEXT NOT NULL DEFAULT '',
        format TEXT NOT NULL DEFAULT '',
        size_bytes INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_tracks_hash ON tracks(hash);
    CREATE INDEX idx_tracks_added ON tracks(added_at);
    CREATE INDEX idx_tracks_last_played ON tracks(last_played);
    CREATE TABLE track_edits (
        track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        title TEXT, artist TEXT, album TEXT, album_artist TEXT, track_no INTEGER, year INTEGER
    );
    CREATE TABLE playlists (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE playlist_tracks (
        playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        PRIMARY KEY (playlist_id, position)
    );
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE play_history (
        id INTEGER PRIMARY KEY,
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        played_at INTEGER NOT NULL
    );",
    // 2: import jobs
    "CREATE TABLE import_jobs (
        id INTEGER PRIMARY KEY,
        url TEXT NOT NULL,
        status TEXT NOT NULL,
        progress REAL NOT NULL DEFAULT 0,
        message TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL
    );",
    // 3: Susic server integration (unified local + server library)
    "ALTER TABLE tracks ADD COLUMN remote_id INTEGER;
     ALTER TABLE tracks ADD COLUMN cache_path TEXT;
     ALTER TABLE tracks ADD COLUMN cache_pinned INTEGER NOT NULL DEFAULT 0;
     CREATE INDEX idx_tracks_remote ON tracks(remote_id);
     ALTER TABLE playlists ADD COLUMN remote_uid TEXT;
     CREATE TABLE playlist_tombstones (uid TEXT PRIMARY KEY, deleted_at INTEGER NOT NULL);",
];

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: usize =
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
pub const LATEST: usize = MIGRATIONS.len();
