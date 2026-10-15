use serde::{Deserialize, Serialize};

pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "wav", "opus", "ogg", "oga"];

pub fn is_audio_path(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_ms: i64,
    /// Absolute path or `content://` URI.
    pub path: String,
    pub source: String,
    pub artwork: Option<String>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub year: Option<i64>,
    pub added_at: i64,
    pub play_count: i64,
    pub last_played: Option<i64>,
    pub favourite: bool,
    pub has_lyrics: bool,
    pub hash: String,
    pub format: String,
    pub size_bytes: i64,
    /// Id of this song on the paired Susic server, if it exists there.
    pub remote_id: Option<i64>,
    /// Offline copy of a server track (absolute path), if downloaded/cached.
    pub cache_path: Option<String>,
    pub cache_pinned: bool,
    /// `local`, `local+server`, `server` or `downloaded`.
    pub availability: String,
}

/// Metadata extracted from a file before it is stored.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NewTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_ms: i64,
    pub path: String,
    pub source: String,
    pub artwork: Option<String>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub year: Option<i64>,
    pub lyrics: Option<String>,
    pub hash: String,
    pub format: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlbumSummary {
    pub album: String,
    pub artist: String,
    pub artwork: Option<String>,
    pub track_count: i64,
    pub duration_ms: i64,
    pub year: Option<i64>,
    pub favourite_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtistSummary {
    pub artist: String,
    pub artwork: Option<String>,
    pub track_count: i64,
    pub album_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
    pub duration_ms: i64,
    pub artwork: Option<String>,
    pub updated_at: i64,
}

pub fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}
