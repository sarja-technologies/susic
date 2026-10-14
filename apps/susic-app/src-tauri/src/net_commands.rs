//! yt-dlp import (desktop) and paired-device transfer commands.

use crate::commands::{free_space, Ctl, Extras};
use crate::controller::Controller;
use crate::importer;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use susic_library::{ImportJob, Upsert};
use susic_sync::{Manifest, Provider, ServerEvent};
use tauri::State;

type R<T> = Result<T, String>;

// ---- yt-dlp -------------------------------------------------------------------

#[tauri::command]
pub fn ytdlp_status() -> serde_json::Value {
    if cfg!(target_os = "android") {
        return json!({ "supported": false, "ytdlp": null, "ffmpeg": null });
    }
    let s = susic_import::ytdlp::tool_status();
    json!({ "supported": true, "ytdlp": s.ytdlp, "ffmpeg": s.ffmpeg })
}

#[derive(Deserialize)]
pub struct UrlImport {
    url: String,
    format: String,
    quality: u32,
    dest_dir: Option<String>,
}

#[tauri::command]
pub fn import_url(c: Ctl, ex: State<Extras>, req: UrlImport) -> R<i64> {
    if cfg!(target_os = "android") {
        return Err(
            "URL import is not available on Android. Transfer music from Susic Desktop instead."
                .into(),
        );
    }
    susic_import::ytdlp::validate_url(&req.url)?;
    let dest = req
        .dest_dir
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            c.with_lib(|l| l.get_setting("import_dir"))
                .ok()
                .flatten()
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| c.music_dir.clone());
    // Duplicate URL guard: skip if the same URL is queued/running.
    let dup = c
        .with_lib(|l| l.import_jobs())?
        .iter()
        .any(|j| j.url == req.url && (j.status == "queued" || j.status == "running"));
    if dup {
        return Err("This URL is already in the import queue".into());
    }
    let id = c.with_lib(|l| l.add_import_job(&req.url))?;
    let cancel = Arc::new(AtomicBool::new(false));
    ex.job_cancels.lock().unwrap().insert(id, cancel.clone());
    let ctl = c.inner().clone();
    std::thread::spawn(move || run_job(ctl, id, req.url, req.format, req.quality, dest, cancel));
    c.emit("jobs://changed", json!({}));
    Ok(id)
}

fn run_job(
    c: Arc<Controller>,
    id: i64,
    url: String,
    format: String,
    quality: u32,
    dest: PathBuf,
    cancel: Arc<AtomicBool>,
) {
    let set = |status: &str, p: f64, msg: &str| {
        let _ = c.with_lib(|l| l.update_import_job(id, status, p, msg));
        c.emit("jobs://changed", json!({}));
    };
    set("running", 0.0, "Starting…");
    let opts = susic_import::ytdlp::Options {
        dest_dir: dest,
        format,
        quality,
    };
    let mut last = std::time::Instant::now();
    let res = susic_import::ytdlp::run(&url, &opts, cancel.clone(), &mut |ev| {
        if let susic_import::ytdlp::Event::Progress(p, line) = ev {
            if last.elapsed().as_millis() > 300 {
                last = std::time::Instant::now();
                let _ = c.with_lib(|l| l.update_import_job(id, "running", p, &line));
                c.emit("jobs://changed", json!({}));
            }
        }
    });
    match res {
        Ok(files) => {
            let sum = importer::run_import(&c, files, "yt-dlp", true);
            if sum.added + sum.updated == 0 && sum.duplicates > 0 {
                set("done", 1.0, "Already in your library");
            } else if !sum.failed.is_empty() && sum.added == 0 {
                set("failed", 1.0, &sum.failed[0]);
            } else {
                set(
                    "done",
                    1.0,
                    &format!("Imported {} track(s)", sum.added + sum.updated),
                );
            }
            c.emit("library://changed", json!({ "reason": "import" }));
            c.emit("import://notify", json!({ "message": "Import complete" }));
        }
        Err(e) if e == "cancelled" => set("cancelled", 0.0, "Cancelled"),
        Err(e) => set("failed", 0.0, &e),
    }
}

#[tauri::command]
pub fn list_jobs(c: Ctl) -> R<Vec<ImportJob>> {
    c.with_lib(|l| l.import_jobs())
}

#[tauri::command]
pub fn cancel_job(ex: State<Extras>, id: i64) {
    if let Some(f) = ex.job_cancels.lock().unwrap().get(&id) {
        f.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn clear_jobs(c: Ctl) -> R<()> {
    c.with_lib(|l| l.clear_finished_jobs())?;
    c.emit("jobs://changed", json!({}));
    Ok(())
}

// ---- sender ---------------------------------------------------------------------

struct LibProvider(Arc<Controller>);

impl Provider for LibProvider {
    fn manifest(&self) -> Manifest {
        let l = self.0.lib.lock().unwrap();
        let tracks = l.list_tracks(&Default::default()).unwrap_or_default();
        let playlists = l
            .playlists()
            .unwrap_or_default()
            .into_iter()
            .map(|p| susic_sync::RemotePlaylist {
                track_ids: l.playlist_track_ids(p.id).unwrap_or_default(),
                name: p.name,
            })
            .collect();
        Manifest {
            device: hostname(),
            tracks: tracks
                .into_iter()
                .filter(|t| !t.path.starts_with("content://"))
                .map(|t| susic_sync::RemoteTrack {
                    id: t.id,
                    title: t.title,
                    artist: t.artist,
                    album: t.album,
                    album_artist: t.album_artist,
                    duration_ms: t.duration_ms,
                    track_no: t.track_no,
                    disc_no: t.disc_no,
                    year: t.year,
                    size_bytes: t.size_bytes,
                    hash: t.hash,
                    format: t.format,
                    has_artwork: t.artwork.is_some(),
                })
                .collect(),
            playlists,
        }
    }
    fn open(&self, id: i64) -> std::io::Result<(std::fs::File, u64)> {
        let t = self
            .0
            .lib
            .lock()
            .unwrap()
            .get_track(id)
            .ok()
            .flatten()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "unknown track"))?;
        let f = std::fs::File::open(&t.path)?;
        let len = f.metadata()?.len();
        Ok((f, len))
    }
    fn artwork(&self, id: i64) -> Option<Vec<u8>> {
        let t = self.0.lib.lock().unwrap().get_track(id).ok().flatten()?;
        std::fs::read(t.artwork?).ok()
    }
    fn lyrics(&self, id: i64) -> Option<String> {
        self.0
            .lib
            .lock()
            .unwrap()
            .get_lyrics(id)
            .ok()
            .and_then(|(l, _)| l)
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "Susic device".into())
}

#[derive(Serialize)]
pub struct ShareInfo {
    code: String,
    host: String,
    port: u16,
    uri: String,
    qr_svg: String,
}

#[tauri::command]
pub fn share_start(c: Ctl, ex: State<Extras>) -> R<ShareInfo> {
    if cfg!(target_os = "android") {
        return Err("Sharing is only available from Susic Desktop".into());
    }
    if let Some(old) = ex.share.lock().unwrap().take() {
        old.stop();
    }
    let ctl = c.inner().clone();
    let emit_ctl = c.inner().clone();
    let server = susic_sync::serve(
        Arc::new(LibProvider(ctl)),
        Duration::from_secs(15 * 60),
        move |ev| {
            emit_ctl.emit(
                "share://event",
                serde_json::to_value(&ev).unwrap_or_default(),
            );
            let _ = matches!(ev, ServerEvent::Expired);
        },
    )
    .map_err(|e| e.to_string())?;
    let host = susic_sync::local_ip()
        .ok_or("No network connection found. Connect to a Wi-Fi or Ethernet network first.")?;
    let uri = susic_sync::pairing_uri(&host, server.port, &server.code);
    let qr = qrcode::QrCode::new(uri.as_bytes()).map_err(|e| e.to_string())?;
    let svg = qr
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .build();
    let info = ShareInfo {
        code: susic_sync::format_code(&server.code),
        host,
        port: server.port,
        uri,
        qr_svg: svg,
    };
    *ex.share.lock().unwrap() = Some(server);
    Ok(info)
}

#[tauri::command]
pub fn share_stop(ex: State<Extras>) {
    if let Some(s) = ex.share.lock().unwrap().take() {
        s.stop();
    }
}

// ---- receiver -------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ConnectReq {
    host: Option<String>,
    port: Option<u16>,
    code: Option<String>,
    uri: Option<String>,
}

#[tauri::command]
pub async fn sync_connect(ex: State<'_, Extras>, req: ConnectReq) -> R<Manifest> {
    let (host, port, code) = match req.uri.as_deref().and_then(susic_sync::parse_pairing_uri) {
        Some(t) => t,
        None => (
            req.host.ok_or("Enter the desktop's address")?,
            req.port.ok_or("Enter the desktop's port")?,
            req.code.ok_or("Enter the pairing code")?,
        ),
    };
    let (client, manifest) = tauri::async_runtime::spawn_blocking(move || -> R<_> {
        let mut c =
            susic_sync::Client::connect(host.trim(), port, &code).map_err(|e| e.to_string())?;
        let m = c.manifest().map_err(|e| e.to_string())?;
        Ok((c, m))
    })
    .await
    .map_err(|e| e.to_string())??;
    *ex.client.lock().unwrap() = Some(client);
    Ok(manifest)
}

#[tauri::command]
pub fn sync_disconnect(ex: State<Extras>) {
    if let Some(c) = ex.client.lock().unwrap().take() {
        c.close();
    }
}

#[derive(Deserialize)]
pub struct ReceiveReq {
    tracks: Vec<susic_sync::RemoteTrack>,
    playlists: Vec<susic_sync::RemotePlaylist>,
}

#[tauri::command]
pub fn sync_receive(c: Ctl, ex: State<Extras>, req: ReceiveReq) -> R<()> {
    if ex.receiving.swap(true, Ordering::SeqCst) {
        return Err("A transfer is already running".into());
    }
    let Some(client) = ex.client.lock().unwrap().take() else {
        ex.receiving.store(false, Ordering::SeqCst);
        return Err("Not connected to a desktop".into());
    };
    let need: i64 = req
        .tracks
        .iter()
        .filter(|t| {
            c.with_lib(|l| l.track_by_hash(&t.hash))
                .ok()
                .flatten()
                .is_none()
        })
        .map(|t| t.size_bytes)
        .sum();
    let free = free_space(&c.music_dir) as i64;
    if need + 50 * 1024 * 1024 > free {
        ex.receiving.store(false, Ordering::SeqCst);
        *ex.client.lock().unwrap() = Some(client);
        return Err(format!(
            "Not enough storage: {:.0} MB needed, {:.0} MB free.",
            need as f64 / 1e6,
            free as f64 / 1e6
        ));
    }
    let ctl = c.inner().clone();
    let flag = ex.receiving.clone();
    c.cancel_scan.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let r = receive(&ctl, client, req);
        flag.store(false, Ordering::SeqCst);
        ctl.emit(
            "sync://done",
            match r {
                Ok(v) => v,
                Err(e) => json!({ "error": e }),
            },
        );
        ctl.emit("library://changed", json!({ "reason": "sync" }));
    });
    Ok(())
}

fn receive(
    c: &Arc<Controller>,
    mut client: susic_sync::Client,
    req: ReceiveReq,
) -> Result<serde_json::Value, String> {
    let dir = c.music_dir.join("received");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let total = req.tracks.len();
    let (mut added, mut skipped, mut failed) = (0usize, 0usize, Vec::<String>::new());
    let mut id_map = std::collections::HashMap::<i64, i64>::new();
    for (i, t) in req.tracks.iter().enumerate() {
        if c.cancel_scan.load(Ordering::Relaxed) {
            break;
        }
        let emit_progress = |frac: f64| {
            c.emit(
                "sync://progress",
                json!({ "index": i, "total": total, "title": t.title, "fraction": frac }),
            );
        };
        if let Some(existing) = c.with_lib(|l| l.track_by_hash(&t.hash)).ok().flatten() {
            id_map.insert(t.id, existing);
            skipped += 1;
            emit_progress(1.0);
            continue;
        }
        let ext = if t.format.is_empty() {
            "audio"
        } else {
            &t.format
        };
        let final_path = dir.join(format!("{}.{}", t.hash, ext));
        let part = dir.join(format!("{}.{}.part", t.hash, ext));
        let offset = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        let res = (|| -> Result<(), String> {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&part)
                .map_err(|e| e.to_string())?;
            let cancel = AtomicBool::new(false);
            client
                .fetch(
                    t.id,
                    offset,
                    &mut f,
                    |done, total| emit_progress(done as f64 / total.max(1) as f64),
                    &cancel,
                )
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(e) = res {
            failed.push(format!("{}: {e}", t.title));
            // Connection state is unknown after a failed stream; stop here, resumable later.
            break;
        }
        // Verify before accepting.
        let (hash, _) = susic_import::fingerprint(&part).map_err(|e| e.to_string())?;
        if hash != t.hash {
            let _ = std::fs::remove_file(&part);
            failed.push(format!("{}: checksum mismatch, discarded", t.title));
            continue;
        }
        std::fs::rename(&part, &final_path).map_err(|e| e.to_string())?;
        match susic_import::read_track(&final_path, &c.artwork_dir, "transfer") {
            Ok(mut nt) => {
                if nt.artwork.is_none() && t.has_artwork {
                    if let Ok(Some(bytes)) = client.artwork(t.id) {
                        nt.artwork = susic_import::save_thumbnail(&bytes, &c.artwork_dir);
                    }
                }
                if nt.lyrics.is_none() {
                    nt.lyrics = client.lyrics(t.id).ok().flatten();
                }
                match c.with_lib(|l| l.upsert_track(&nt)) {
                    Ok(Upsert::Inserted(id))
                    | Ok(Upsert::Updated(id))
                    | Ok(Upsert::Duplicate(id)) => {
                        id_map.insert(t.id, id);
                        added += 1;
                    }
                    Err(e) => failed.push(format!("{}: {e}", t.title)),
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&final_path);
                failed.push(e.to_string());
            }
        }
    }
    for p in &req.playlists {
        let ids: Vec<i64> = p
            .track_ids
            .iter()
            .filter_map(|i| id_map.get(i).copied())
            .collect();
        if ids.is_empty() {
            continue;
        }
        let _ = c.with_lib(|l| {
            let id = l.create_playlist(&p.name)?;
            l.add_to_playlist(id, &ids)?;
            Ok(())
        });
    }
    client.close();
    Ok(json!({ "added": added, "skipped": skipped, "failed": failed }))
}

// ---- YouTube discovery -------------------------------------------------------

#[derive(Serialize)]
pub struct YtResult {
    id: String,
    title: String,
    channel: String,
    thumb: String,
}

/// Search via the official YouTube Data API using the user's own key. Results
/// are returned transiently and never stored; they link out to YouTube.
#[tauri::command]
pub async fn youtube_search(c: Ctl<'_>, query: String) -> R<Vec<YtResult>> {
    let (enabled, key) = c.with_lib(|l| {
        Ok((
            l.get_setting("youtube_enabled")?,
            l.get_setting("youtube_api_key")?,
        ))
    })?;
    if enabled.as_deref() != Some("true") {
        return Err("YouTube discovery is turned off in Settings".into());
    }
    let key = key
        .filter(|k| !k.trim().is_empty())
        .ok_or("Add your YouTube Data API key in Settings")?;
    tauri::async_runtime::spawn_blocking(move || -> R<Vec<YtResult>> {
        let resp = ureq::get("https://www.googleapis.com/youtube/v3/search")
            .query("part", "snippet")
            .query("type", "video")
            .query("videoCategoryId", "10")
            .query("maxResults", "10")
            .query("q", &query)
            .query("key", key.trim())
            .timeout(Duration::from_secs(10))
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(403, _) => {
                    "YouTube refused the request — check your API key and quota".to_string()
                }
                ureq::Error::Status(400, _) => "YouTube rejected the API key".to_string(),
                e => format!("YouTube request failed: {e}"),
            })?;
        let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        Ok(v["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| {
                        Some(YtResult {
                            id: i["id"]["videoId"].as_str()?.to_string(),
                            title: i["snippet"]["title"].as_str()?.to_string(),
                            channel: i["snippet"]["channelTitle"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            thumb: i["snippet"]["thumbnails"]["default"]["url"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    })
    .await
    .map_err(|e| e.to_string())?
}
