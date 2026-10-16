use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Address to listen on. Prefer a Tailscale/LAN address or `127.0.0.1`
    /// behind `tailscale serve`; see docs/SERVER.md.
    pub bind: String,
    /// Directory holding the music. Susic never changes or deletes files here;
    /// the only writes are new imports into `imports_dir` when `ytdlp_enabled`.
    pub music_dir: PathBuf,
    /// Holds the SQLite database and artwork thumbnails.
    pub data_dir: PathBuf,
    /// Seconds between automatic rescans (0 disables).
    pub scan_interval_secs: u64,
    /// Display name shown in clients.
    pub server_name: String,
    /// Allow paired devices to search YouTube and import audio through this
    /// server's yt-dlp + FFmpeg. Off by default.
    pub ytdlp_enabled: bool,
    /// Sub-directory of `music_dir` that receives imports.
    pub imports_dir: String,
    /// Hosts imports may come from (subdomains included). `["*"]` allows any
    /// http(s) URL yt-dlp supports — including hosts on your LAN.
    pub import_hosts: Vec<String>,
    /// Imports are refused when the imports filesystem has less free space.
    pub imports_min_free_mb: u64,
    /// Maximum queued + running imports.
    pub imports_max_queue: usize,
    /// A single import is stopped after this many minutes.
    pub imports_max_minutes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8484".into(),
            music_dir: PathBuf::from("/srv/music"),
            data_dir: PathBuf::from("/var/lib/susic"),
            scan_interval_secs: 300,
            server_name: "Susic Server".into(),
            ytdlp_enabled: false,
            imports_dir: "Susic Imports".into(),
            import_hosts: vec![
                "youtube.com".into(),
                "youtu.be".into(),
                "youtube-nocookie.com".into(),
            ],
            imports_min_free_mb: 2048,
            imports_max_queue: 20,
            imports_max_minutes: 120,
        }
    }
}

impl Config {
    /// Load from an optional TOML file, then apply `SUSIC_*` environment overrides.
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let mut cfg = match path {
            Some(p) => {
                let text = std::fs::read_to_string(p)
                    .map_err(|e| format!("cannot read config {}: {e}", p.display()))?;
                toml::from_str(&text).map_err(|e| format!("invalid config {}: {e}", p.display()))?
            }
            None => Config::default(),
        };
        if let Ok(v) = std::env::var("SUSIC_BIND") {
            cfg.bind = v;
        }
        if let Ok(v) = std::env::var("SUSIC_MUSIC_DIR") {
            cfg.music_dir = v.into();
        }
        if let Ok(v) = std::env::var("SUSIC_DATA_DIR") {
            cfg.data_dir = v.into();
        }
        if let Ok(v) = std::env::var("SUSIC_SCAN_INTERVAL") {
            cfg.scan_interval_secs = v
                .parse()
                .map_err(|_| "SUSIC_SCAN_INTERVAL must be a number")?;
        }
        if let Ok(v) = std::env::var("SUSIC_YTDLP_ENABLED") {
            cfg.ytdlp_enabled = matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        if let Ok(v) = std::env::var("SUSIC_IMPORTS_DIR") {
            cfg.imports_dir = v;
        }
        let rel = Path::new(&cfg.imports_dir);
        if cfg.imports_dir.trim().is_empty()
            || rel.is_absolute()
            || rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(
                "imports_dir must be a relative sub-directory of music_dir (no '..')".into(),
            );
        }
        if cfg.imports_max_queue == 0 || cfg.imports_max_minutes == 0 {
            return Err("imports_max_queue and imports_max_minutes must be at least 1".into());
        }
        Ok(cfg)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("susic-server.db")
    }
    pub fn artwork_dir(&self) -> PathBuf {
        self.data_dir.join("artwork")
    }
    /// Where `susic-server ytdlp-install` puts the managed yt-dlp.
    pub fn tools_dir(&self) -> PathBuf {
        self.data_dir.join("tools")
    }
    pub fn imports_path(&self) -> PathBuf {
        self.music_dir.join(&self.imports_dir)
    }
}
