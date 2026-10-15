//! Client for the optional Susic server (see `docs/SERVER.md`).

mod reader;
pub use reader::HttpRangeReader;

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Server unreachable (offline, DNS, timeout, connection reset).
    #[error("server unreachable: {0}")]
    Unreachable(String),
    #[error("not authorised (token revoked or invalid)")]
    Unauthorized,
    #[error("{0}")]
    Server(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancelled")]
    Cancelled,
}
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn is_transient(&self) -> bool {
        matches!(self, Error::Unreachable(_))
    }
}

fn map_err(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Status(401, _) => Error::Unauthorized,
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["error"].as_str().map(String::from))
                .unwrap_or_else(|| format!("HTTP {code}"));
            if code >= 500 {
                Error::Unreachable(msg)
            } else {
                Error::Server(msg)
            }
        }
        ureq::Error::Transport(t) => Error::Unreachable(t.to_string()),
    }
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(20))
        .build()
}

pub fn normalise_base(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    if u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else {
        format!("http://{u}")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Health {
    pub status: String,
    pub name: String,
    pub version: String,
    pub api: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RemoteTrack {
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

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDelta {
    pub revision: i64,
    pub tracks: Vec<RemoteTrack>,
    pub removed: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RemotePlaylist {
    pub uid: String,
    pub name: String,
    pub updated_at: i64,
    pub deleted: bool,
    pub track_ids: Vec<i64>,
    pub rev: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerStatus {
    pub name: String,
    pub version: String,
    pub tracks: i64,
    pub bytes: i64,
    pub revision: i64,
    pub scanning: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Paired {
    pub token: String,
    pub device_id: i64,
    pub server_name: String,
}

#[derive(Debug, Clone)]
pub struct Server {
    pub base: String,
    pub token: String,
}

pub fn health(base: &str) -> Result<Health> {
    agent()
        .get(&format!("{}/health", normalise_base(base)))
        .timeout(Duration::from_secs(5))
        .call()
        .map_err(map_err)?
        .into_json()
        .map_err(|e| Error::Server(e.to_string()))
}

pub fn pair(base: &str, code: &str, device_name: &str) -> Result<Paired> {
    agent()
        .post(&format!("{}/api/v1/pair", normalise_base(base)))
        .send_json(serde_json::json!({ "code": code, "device_name": device_name }))
        .map_err(map_err)?
        .into_json()
        .map_err(|e| Error::Server(e.to_string()))
}

impl Server {
    pub fn new(base: &str, token: &str) -> Self {
        Self {
            base: normalise_base(base),
            token: token.to_string(),
        }
    }

    fn get(&self, path: &str) -> ureq::Request {
        agent()
            .get(&format!("{}/api/v1{}", self.base, path))
            .set("Authorization", &format!("Bearer {}", self.token))
    }

    pub fn status(&self) -> Result<ServerStatus> {
        self.get("/status")
            .call()
            .map_err(map_err)?
            .into_json()
            .map_err(|e| Error::Server(e.to_string()))
    }

    pub fn library(&self, since: i64) -> Result<LibraryDelta> {
        self.get(&format!("/library?since={since}"))
            .timeout(Duration::from_secs(120))
            .call()
            .map_err(map_err)?
            .into_json()
            .map_err(|e| Error::Server(e.to_string()))
    }

    pub fn playlists(&self) -> Result<Vec<RemotePlaylist>> {
        #[derive(Deserialize)]
        struct R {
            playlists: Vec<RemotePlaylist>,
        }
        let r: R = self
            .get("/playlists")
            .call()
            .map_err(map_err)?
            .into_json()
            .map_err(|e| Error::Server(e.to_string()))?;
        Ok(r.playlists)
    }

    /// Returns the version now stored on the server (which may be newer than ours).
    pub fn put_playlist(
        &self,
        uid: &str,
        name: &str,
        track_ids: &[i64],
        updated_at: i64,
    ) -> Result<RemotePlaylist> {
        let req = agent()
            .put(&format!("{}/api/v1/playlists/{}", self.base, uid))
            .set("Authorization", &format!("Bearer {}", self.token));
        match req.send_json(
            serde_json::json!({ "name": name, "track_ids": track_ids, "updated_at": updated_at }),
        ) {
            Ok(r) => r.into_json().map_err(|e| Error::Server(e.to_string())),
            Err(ureq::Error::Status(409, r)) => {
                r.into_json().map_err(|e| Error::Server(e.to_string()))
            }
            Err(e) => Err(map_err(e)),
        }
    }

    pub fn delete_playlist(&self, uid: &str, updated_at: i64) -> Result<()> {
        agent()
            .delete(&format!(
                "{}/api/v1/playlists/{}?updated_at={}",
                self.base, uid, updated_at
            ))
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .map(|_| ())
            .or_else(|e| match e {
                ureq::Error::Status(404, _) => Ok(()),
                e => Err(map_err(e)),
            })
    }

    pub fn set_favourite(&self, id: i64, fav: bool) -> Result<()> {
        agent()
            .put(&format!("{}/api/v1/tracks/{}/favourite", self.base, id))
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(serde_json::json!({ "favourite": fav }))
            .map(|_| ())
            .map_err(map_err)
    }

    pub fn stream_url(&self, id: i64) -> String {
        format!(
            "{}/api/v1/tracks/{}/stream?access_token={}",
            self.base, id, self.token
        )
    }

    pub fn lyrics(&self, id: i64) -> Option<String> {
        self.get(&format!("/tracks/{id}/lyrics"))
            .call()
            .ok()?
            .into_string()
            .ok()
    }

    pub fn fetch_artwork(&self, id: i64, dest: &Path) -> Result<()> {
        let r = self
            .get(&format!("/tracks/{id}/artwork"))
            .call()
            .map_err(map_err)?;
        let mut buf = Vec::new();
        r.into_reader()
            .take(8 * 1024 * 1024)
            .read_to_end(&mut buf)?;
        std::fs::write(dest, buf)?;
        Ok(())
    }

    /// Download track `id` to `dest`, resuming from `dest.part` if present.
    /// `progress(done, total)` returns false to cancel. Retries transient
    /// failures; the partial file is kept so a later call resumes.
    pub fn download(
        &self,
        id: i64,
        dest: &Path,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<()> {
        let part = dest.with_extension(format!(
            "{}.part",
            dest.extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut attempts = 0;
        loop {
            let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            let mut req = self.get(&format!("/tracks/{id}/stream"));
            if have > 0 {
                req = req.set("Range", &format!("bytes={have}-"));
            }
            let outcome: Result<()> = (|| {
                let resp = req.call().map_err(map_err)?;
                let status = resp.status();
                let total_hdr = resp
                    .header("Content-Range")
                    .and_then(|c| c.rsplit('/').next().and_then(|n| n.parse::<u64>().ok()))
                    .or_else(|| {
                        resp.header("Content-Length")
                            .and_then(|n| n.parse::<u64>().ok())
                            .map(|n| n + if status == 206 { have } else { 0 })
                    });
                let mut f = if status == 206 {
                    std::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&part)?
                } else {
                    std::fs::File::create(&part)? // server ignored Range: restart
                };
                let mut done = if status == 206 { have } else { 0 };
                let total = total_hdr.unwrap_or(0);
                let mut r = resp.into_reader();
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = r
                        .read(&mut buf)
                        .map_err(|e| Error::Unreachable(e.to_string()))?;
                    if n == 0 {
                        break;
                    }
                    f.write_all(&buf[..n])?;
                    done += n as u64;
                    if !progress(done, total) {
                        return Err(Error::Cancelled);
                    }
                }
                f.flush()?;
                if total > 0 && done < total {
                    return Err(Error::Unreachable("connection closed early".into()));
                }
                Ok(())
            })();
            match outcome {
                Ok(()) => {
                    std::fs::rename(&part, dest)?;
                    return Ok(());
                }
                Err(e) if e.is_transient() && attempts < 5 => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(500 * (1 << attempts)));
                }
                Err(e) => return Err(e),
            }
        }
    }
}
