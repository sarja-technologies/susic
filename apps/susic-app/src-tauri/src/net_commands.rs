//! Paired-device transfer commands (YouTube search/import lives in `youtube.rs`).

use crate::commands::{free_space, Ctl, Extras};
use crate::controller::Controller;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use susic_library::Upsert;
use susic_sync::{Manifest, Provider, ServerEvent};
use tauri::State;

type R<T> = Result<T, String>;

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
