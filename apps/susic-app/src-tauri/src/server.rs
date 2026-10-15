//! Optional Susic server connection: pairing, library/playlist/favourite sync,
//! offline downloads and the online monitor. Everything here is inert until the
//! user pairs a server; local playback never depends on it.

use crate::commands::Ctl;
use crate::controller::Controller;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use susic_client::{Error as ClientError, Server};
use susic_core::NewTrack;

type R<T> = Result<T, String>;

#[derive(Serialize, Deserialize, Clone)]
struct Creds {
    url: String,
    token: String,
    name: String,
    device_name: String,
}

fn creds_path(c: &Controller) -> PathBuf {
    c.data_dir.join("server.json")
}

/// Credentials live in their own file (mode 0600), not in `library.db`, so a
/// copied or exported library never carries the access token.
fn save_creds(c: &Controller, creds: &Creds) -> R<()> {
    let p = creds_path(c);
    let json = serde_json::to_vec_pretty(creds).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&p)
            .map_err(|e| e.to_string())?;
        f.write_all(&json).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    std::fs::write(&p, json).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_creds(c: &Controller) -> Option<Creds> {
    serde_json::from_slice(&std::fs::read(creds_path(c)).ok()?).ok()
}

fn creds_name(c: &Controller) -> Option<(String, String, String)> {
    load_creds(c).map(|k| (k.url, k.name, k.device_name))
}

fn downloads_dir(c: &Controller) -> PathBuf {
    c.data_dir.join("downloads")
}

fn server_of(c: &Controller) -> R<Server> {
    c.server.lock().unwrap().clone().ok_or_else(|| "No Susic server is connected".to_string())
}

fn setting_i64(c: &Controller, key: &str, default: i64) -> i64 {
    c.with_lib(|l| l.get_setting(key)).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(default)
}

// ---- startup / monitor -------------------------------------------------------

pub fn init(c: &Arc<Controller>) {
    if let Some(k) = load_creds(c) {
        *c.server.lock().unwrap() = Some(Server::new(&k.url, &k.token));
    }
    let c = c.clone();
    std::thread::Builder::new()
        .name("susic-server-monitor".into())
        .spawn(move || monitor(c))
        .ok();
}

fn monitor(c: Arc<Controller>) {
    let mut first = true;
    let mut last_online: Option<bool> = None;
    loop {
        let srv = c.server.lock().unwrap().clone();
        if let Some(srv) = srv {
            let online = susic_client::health(&srv.base).is_ok();
            c.server_online.store(online, Ordering::Relaxed);
            if last_online != Some(online) {
                c.emit("server://state", json!({ "online": online }));
            }
            if online && (first || last_online == Some(false) || tick_due()) {
                if let Err(e) = sync_all(&c) {
                    c.emit("server://error", json!({ "message": e }));
                }
            }
            last_online = Some(online);
            first = false;
        } else {
            first = true;
            last_online = None;
        }
        std::thread::sleep(Duration::from_secs(15));
    }
}

fn tick_due() -> bool {
    static N: OnceLock<Mutex<u32>> = OnceLock::new();
    let mut n = N.get_or_init(|| Mutex::new(0)).lock().unwrap();
    *n += 1;
    *n % 4 == 0 // every ~minute
}

// ---- sync --------------------------------------------------------------------

static SYNC_LOCK: Mutex<()> = Mutex::new(());

pub fn sync_all(c: &Arc<Controller>) -> R<()> {
    let _g = SYNC_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let srv = server_of(c)?;
    sync_library(c, &srv).map_err(stringify)?;
    sync_playlists(c, &srv).map_err(stringify)?;
    c.emit("library://changed", json!({ "reason": "sync" }));
    fetch_artwork(c, &srv);
    Ok(())
}

fn stringify(e: ClientError) -> String {
    e.to_string()
}

fn db<T>(r: susic_library::Result<T>) -> Result<T, ClientError> {
    r.map_err(|e| ClientError::Server(e.to_string()))
}

fn sync_library(c: &Arc<Controller>, srv: &Server) -> Result<(), ClientError> {
    let have = c.with_lib(|l| l.stats()).map(|s| s.tracks).unwrap_or(0);
    let mut since = setting_i64(c, "server.revision", 0);
    if have == 0 {
        since = 0;
    }
    let delta = srv.library(since)?;
    let first_sync = since == 0;
    let mut to_push_fav = Vec::new();
    {
        let l = c.lib.lock().unwrap();
        for t in &delta.tracks {
            let nt = NewTrack {
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                album_artist: t.album_artist.clone(),
                duration_ms: t.duration_ms,
                track_no: t.track_no,
                disc_no: t.disc_no,
                year: t.year,
                format: t.format.clone(),
                size_bytes: t.size_bytes,
                hash: t.hash.clone(),
                source: "server".into(),
                ..Default::default()
            };
            let id = db(l.upsert_remote(&nt, t.id))?;
            let local_fav = l.get_track(id).ok().flatten().map(|x| x.favourite).unwrap_or(false);
            if first_sync && local_fav && !t.favourite {
                to_push_fav.push(t.id); // adopt local favourites on first link
            } else if local_fav != t.favourite {
                db(l.set_favourite(id, t.favourite))?;
            }
        }
        db(l.remote_removed(&delta.removed))?;
        db(l.set_setting("server.revision", &delta.revision.to_string()))?;
    }
    for rid in to_push_fav {
        let _ = srv.set_favourite(rid, true);
    }
    Ok(())
}

fn fetch_artwork(c: &Arc<Controller>, srv: &Server) {
    let cands = c.with_lib(|l| l.server_art_candidates()).unwrap_or_default();
    let mut n = 0;
    for (id, rid, art) in cands {
        if art.as_deref().map(|p| std::path::Path::new(p).exists()).unwrap_or(false) {
            continue;
        }
        let dest = c.artwork_dir.join(format!("srv-{rid}.jpg"));
        if srv.fetch_artwork(rid, &dest).is_ok() {
            let _ = c.with_lib(|l| l.set_artwork(id, &dest.to_string_lossy()));
            n += 1;
            if n % 25 == 0 {
                c.emit("library://changed", json!({ "reason": "artwork" }));
            }
        } else if !c.server_online.load(Ordering::Relaxed) {
            break;
        }
    }
    if n % 25 != 0 {
        c.emit("library://changed", json!({ "reason": "artwork" }));
    }
}

fn new_uid() -> String {
    let mut b = [0u8; 12];
    // No rand dependency in the app crate: derive from time + address entropy.
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let seed = t.as_nanos() ^ ((&b as *const _ as usize as u128) << 17) ^ std::process::id() as u128;
    for (i, x) in b.iter_mut().enumerate() {
        *x = ((seed >> ((i % 16) * 8)) as u8) ^ (i as u8).wrapping_mul(131);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn sync_playlists(c: &Arc<Controller>, srv: &Server) -> Result<(), ClientError> {
    // 1. Propagate local deletions.
    let tombs = db(c.lib.lock().unwrap().take_playlist_tombstones())?;
    for (uid, at) in tombs {
        srv.delete_playlist(&uid, at)?;
        db(c.lib.lock().unwrap().clear_playlist_tombstone(&uid))?;
    }
    let remote = srv.playlists()?;
    let l = c.lib.lock().unwrap();
    // 2. Apply remote changes that are newer than ours.
    let mut seen = std::collections::HashSet::new();
    for r in &remote {
        seen.insert(r.uid.clone());
        let local = db(l.playlist_by_remote_uid(&r.uid))?;
        if r.deleted {
            if let Some((id, _)) = local {
                db(l.set_playlist_remote_uid(id, ""))?; // avoid a tombstone echo
                db(l.delete_playlist(id))?;
                let _ = l.clear_playlist_tombstone("");
            }
            continue;
        }
        let mapped: Vec<i64> = r
            .track_ids
            .iter()
            .filter_map(|rid| l.track_by_remote(*rid).ok().flatten())
            .collect();
        match local {
            None => {
                let id = db(l.create_playlist(&r.name))?;
                db(l.set_playlist_remote_uid(id, &r.uid))?;
                db(l.set_playlist_order(id, &mapped))?;
                db(l.set_playlist_updated_at(id, r.updated_at))?;
            }
            Some((id, updated)) if r.updated_at > updated => {
                // Keep local-only songs (not on the server) after the synced ones.
                let existing = db(l.playlist_track_ids(id))?;
                let local_only: Vec<i64> = db(l.get_tracks(&existing))?
                    .into_iter()
                    .filter(|t| t.remote_id.is_none())
                    .map(|t| t.id)
                    .collect();
                let mut order = mapped.clone();
                order.extend(local_only);
                db(l.rename_playlist(id, &r.name))?;
                db(l.set_playlist_order(id, &order))?;
                db(l.set_playlist_updated_at(id, r.updated_at))?;
            }
            _ => {}
        }
    }
    // 3. Push local playlists that are new or newer.
    let remote_map: std::collections::HashMap<_, _> = remote.iter().map(|r| (r.uid.clone(), r)).collect();
    let locals = db(l.playlists())?;
    for p in locals {
        let uid = db(l.playlist_remote_uid(p.id))?.filter(|u| !u.is_empty());
        let newer = match uid.as_ref().and_then(|u| remote_map.get(u)) {
            Some(r) => p.updated_at > r.updated_at,
            None => true,
        };
        if !newer {
            continue;
        }
        let ids = db(l.playlist_track_ids(p.id))?;
        let rids: Vec<i64> = db(l.get_tracks(&ids))?.into_iter().filter_map(|t| t.remote_id).collect();
        let uid = uid.unwrap_or_else(new_uid);
        drop_guard_put(srv, &uid, &p.name, &rids, p.updated_at)?;
        db(l.set_playlist_remote_uid(p.id, &uid))?;
    }
    Ok(())
}

fn drop_guard_put(srv: &Server, uid: &str, name: &str, rids: &[i64], at: i64) -> Result<(), ClientError> {
    srv.put_playlist(uid, name, rids, at).map(|_| ())
}

// ---- downloads / cache ---------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct CacheInfo {
    pub bytes: i64,
    pub tracks: usize,
    pub max_mb: i64,
    pub auto: bool,
}

fn cache_info(c: &Controller) -> CacheInfo {
    let rows = c.with_lib(|l| l.cached_tracks()).unwrap_or_default();
    CacheInfo {
        bytes: rows.iter().map(|r| r.2).sum(),
        tracks: rows.len(),
        max_mb: setting_i64(c, "cache.max_mb", 2048),
        auto: c.with_lib(|l| l.get_setting("cache.auto")).ok().flatten().as_deref() == Some("true"),
    }
}

/// Make room for `need` bytes by evicting unpinned (auto-cached) files,
/// least recently played first. Returns false if it still doesn't fit.
fn ensure_room(c: &Controller, need: i64) -> bool {
    let limit = setting_i64(c, "cache.max_mb", 2048) * 1024 * 1024;
    let mut rows = c.with_lib(|l| l.cached_tracks()).unwrap_or_default();
    let mut used: i64 = rows.iter().map(|r| r.2).sum();
    for (id, path, size, pinned) in rows.drain(..) {
        if used + need <= limit {
            break;
        }
        if !pinned {
            let _ = std::fs::remove_file(&path);
            let _ = c.with_lib(|l| l.set_cache(id, None, false));
            used -= size;
        }
    }
    used + need <= limit
}

pub fn download_one(c: &Arc<Controller>, id: i64, pinned: bool) -> R<()> {
    let srv = server_of(c)?;
    let t = c.with_lib(|l| l.get_track(id))?.ok_or("Track not found")?;
    let rid = t.remote_id.ok_or("This song isn't on your server")?;
    if t.source != "server" {
        return Ok(()); // already stored locally
    }
    if let Some(p) = &t.cache_path {
        if std::path::Path::new(p).exists() {
            if pinned && !t.cache_pinned {
                c.with_lib(|l| l.set_cache(id, Some(p), true))?;
            }
            return Ok(());
        }
    }
    if !ensure_room(c, t.size_bytes) {
        return Err("Download limit reached — raise it in Settings or remove downloads".into());
    }
    let ext = if t.format.is_empty() { "audio" } else { t.format.as_str() };
    let dest = downloads_dir(c).join(format!("{rid}.{ext}"));
    srv.download(rid, &dest, &mut |done, total| {
        c.emit("download://progress", json!({ "id": id, "done": done, "total": total }));
        true
    })
    .map_err(|e| e.to_string())?;
    c.with_lib(|l| l.set_cache(id, Some(&dest.to_string_lossy()), pinned))?;
    c.emit("library://changed", json!({ "reason": "download", "id": id }));
    Ok(())
}

/// Called when a streamed track starts and "cache played songs" is on.
pub fn maybe_auto_cache(c: &Arc<Controller>, id: i64) {
    if !cache_info(c).auto {
        return;
    }
    let c = c.clone();
    std::thread::spawn(move || {
        let _ = download_one(&c, id, false);
    });
}

// ---- commands --------------------------------------------------------------------

#[derive(Serialize)]
pub struct ServerInfo {
    configured: bool,
    url: Option<String>,
    name: Option<String>,
    device_name: Option<String>,
    online: bool,
    tracks: i64,
}

#[tauri::command]
pub fn server_info(c: Ctl) -> ServerInfo {
    let creds = creds_name(&c);
    let tracks = c
        .with_lib(|l| Ok(l.list_tracks(&susic_library::TrackQuery::default())?.iter().filter(|t| t.remote_id.is_some()).count() as i64))
        .unwrap_or(0);
    ServerInfo {
        configured: creds.is_some(),
        url: creds.as_ref().map(|c| c.0.clone()),
        name: creds.as_ref().map(|c| c.1.clone()),
        device_name: creds.map(|c| c.2),
        online: c.server_online.load(Ordering::Relaxed),
        tracks,
    }
}

#[derive(Deserialize)]
pub struct ConnectReq {
    url: String,
    code: String,
    device_name: Option<String>,
}

#[tauri::command]
pub async fn server_connect(c: Ctl<'_>, req: ConnectReq) -> R<ServerInfo> {
    let ctl: Arc<Controller> = c.inner().clone();
    let device = req.device_name.filter(|d| !d.trim().is_empty()).unwrap_or_else(|| {
        std::env::var("HOSTNAME").unwrap_or_else(|_| if cfg!(target_os = "android") { "Android phone".into() } else { "Susic desktop".into() })
    });
    let base = susic_client::normalise_base(&req.url);
    let b2 = base.clone();
    let paired = tauri::async_runtime::spawn_blocking(move || {
        susic_client::health(&b2)?;
        susic_client::pair(&b2, &req.code, &device).map(|p| (p, device))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| match e {
        ClientError::Unreachable(m) => format!("Can't reach {base}: {m}"),
        e => e.to_string(),
    })?;
    let (p, device) = paired;
    save_creds(&ctl, &Creds { url: base.clone(), token: p.token.clone(), name: p.server_name, device_name: device })?;
    *ctl.server.lock().unwrap() = Some(Server::new(&base, &p.token));
    ctl.with_lib(|l| l.set_setting("server.revision", "0"))?;
    ctl.server_online.store(true, Ordering::Relaxed);
    ctl.emit("server://state", json!({ "online": true }));
    let c2 = ctl.clone();
    std::thread::spawn(move || {
        if let Err(e) = sync_all(&c2) {
            c2.emit("server://error", json!({ "message": e }));
        }
    });
    Ok(server_info_of(&ctl))
}

fn server_info_of(c: &Arc<Controller>) -> ServerInfo {
    let creds = creds_name(c);
    ServerInfo {
        configured: creds.is_some(),
        url: creds.as_ref().map(|c| c.0.clone()),
        name: creds.as_ref().map(|c| c.1.clone()),
        device_name: creds.map(|c| c.2),
        online: c.server_online.load(Ordering::Relaxed),
        tracks: 0,
    }
}

#[tauri::command]
pub fn server_disconnect(c: Ctl, keep_downloads: bool) -> R<()> {
    *c.server.lock().unwrap() = None;
    c.server_online.store(false, Ordering::Relaxed);
    let _ = std::fs::remove_file(creds_path(&c));
    let files = c.with_lib(|l| l.forget_server(keep_downloads))?;
    for f in files {
        let _ = std::fs::remove_file(f);
    }
    c.with_lib(|l| l.set_setting("server.revision", "0"))?;
    c.emit("server://state", json!({ "online": false }));
    c.emit("library://changed", json!({ "reason": "server-removed" }));
    c.emit("queue://changed", json!({}));
    Ok(())
}

#[tauri::command]
pub async fn server_sync(c: Ctl<'_>) -> R<()> {
    let ctl: Arc<Controller> = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || sync_all(&ctl)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn server_rescan(c: Ctl<'_>) -> R<()> {
    let srv = server_of(&c)?;
    tauri::async_runtime::spawn_blocking(move || {
        let r = ureq::post(&format!("{}/api/v1/scan", srv.base))
            .set("Authorization", &format!("Bearer {}", srv.token))
            .call();
        match r {
            Ok(_) | Err(ureq::Error::Status(409, _)) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn server_status(c: Ctl<'_>) -> R<susic_client::ServerStatus> {
    let srv = server_of(&c)?;
    tauri::async_runtime::spawn_blocking(move || srv.status().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn download_tracks(c: Ctl<'_>, ids: Vec<i64>) -> R<usize> {
    let ctl: Arc<Controller> = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut n = 0;
        for id in ids {
            download_one(&ctl, id, true)?;
            n += 1;
        }
        Ok(n)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn remove_downloads(c: Ctl, ids: Vec<i64>) -> R<()> {
    for id in ids {
        if let Some(t) = c.with_lib(|l| l.get_track(id))? {
            if t.source == "server" {
                if let Some(p) = &t.cache_path {
                    let _ = std::fs::remove_file(p);
                }
                c.with_lib(|l| l.set_cache(id, None, false))?;
            }
        }
    }
    c.emit("library://changed", json!({ "reason": "download" }));
    Ok(())
}

#[tauri::command]
pub fn cache_status(c: Ctl) -> CacheInfo {
    cache_info(&c)
}

#[tauri::command]
pub fn clear_downloads(c: Ctl) -> R<usize> {
    let rows = c.with_lib(|l| l.cached_tracks())?;
    let n = rows.len();
    for (id, p, _, _) in rows {
        let _ = std::fs::remove_file(p);
        c.with_lib(|l| l.set_cache(id, None, false))?;
    }
    c.emit("library://changed", json!({ "reason": "download" }));
    Ok(n)
}

#[tauri::command]
pub async fn server_devices(c: Ctl<'_>) -> R<serde_json::Value> {
    let srv = server_of(&c)?;
    tauri::async_runtime::spawn_blocking(move || {
        ureq::get(&format!("{}/api/v1/devices", srv.base))
            .set("Authorization", &format!("Bearer {}", srv.token))
            .call()
            .map_err(|e| e.to_string())?
            .into_json::<serde_json::Value>()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn server_revoke_device(c: Ctl<'_>, id: i64) -> R<()> {
    let srv = server_of(&c)?;
    tauri::async_runtime::spawn_blocking(move || {
        ureq::delete(&format!("{}/api/v1/devices/{}", srv.base, id))
            .set("Authorization", &format!("Bearer {}", srv.token))
            .call()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, MediaInfo};
    use std::sync::Mutex as StdMutex;

    struct Rec(StdMutex<Vec<MediaInfo>>);
    impl Engine for Arc<Rec> {
        fn load(&self, m: MediaInfo, _: i64, _: bool) {
            self.0.lock().unwrap().push(m);
        }
        fn play(&self) {}
        fn pause(&self) {}
        fn seek(&self, _: i64) {}
        fn set_volume(&self, _: f32) {}
        fn stop(&self) {}
    }

    fn wav(path: &std::path::Path, secs: f32, f: f32) {
        let rate = 8000u32;
        let n = (rate as f32 * secs) as u32;
        let mut d = Vec::new();
        d.extend(b"RIFF");
        d.extend((36 + n * 2).to_le_bytes());
        d.extend(b"WAVEfmt ");
        d.extend(16u32.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(rate.to_le_bytes());
        d.extend((rate * 2).to_le_bytes());
        d.extend(2u16.to_le_bytes());
        d.extend(16u16.to_le_bytes());
        d.extend(b"data");
        d.extend((n * 2).to_le_bytes());
        for i in 0..n {
            let v = (((i as f32) * f).sin() * 8000.0) as i16;
            d.extend(v.to_le_bytes());
        }
        std::fs::write(path, d).unwrap();
    }

    /// Full path: real server on a socket → pair → sync into the app's library →
    /// stream URL resolution → download for offline → local copy preferred.
    #[test]
    fn app_syncs_streams_and_downloads_from_real_server() {
        let root = std::env::temp_dir().join(format!("susic-app-srv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let music = root.join("music");
        std::fs::create_dir_all(&music).unwrap();
        wav(&music.join("Remote Song.wav"), 3.0, 0.07);
        wav(&music.join("Shared Song.wav"), 2.0, 0.11);
        let cfg = susic_server::Config {
            bind: "127.0.0.1:0".into(),
            music_dir: music.clone(),
            data_dir: root.join("srv"),
            scan_interval_secs: 0,
            server_name: "Test".into(),
        };
        let state = susic_server::open_state(cfg).unwrap();
        state.run_scan().unwrap().unwrap();
        let code = susic_server::issue_pair_code(&state, Duration::from_secs(60)).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let st = state.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
            rt.block_on(async move {
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(l.local_addr().unwrap()).unwrap();
                susic_server::serve(st, l, std::future::pending()).await.unwrap();
            });
        });
        let addr = rx.recv().unwrap();
        let base = format!("http://{addr}");

        let emit: crate::controller::Emitter = Arc::new(|_, _| {});
        let ctl = Controller::new(root.join("app"), emit).unwrap();
        let rec = Arc::new(Rec(StdMutex::new(vec![])));
        ctl.set_engine(Box::new(rec.clone()));

        // The user already owns "Shared Song" locally (same bytes as on the server).
        let local_copy = root.join("mine");
        std::fs::create_dir_all(&local_copy).unwrap();
        std::fs::copy(music.join("Shared Song.wav"), local_copy.join("Shared Song.wav")).unwrap();
        let nt = susic_import::read_track(&local_copy.join("Shared Song.wav"), &ctl.artwork_dir, "local").unwrap();
        ctl.with_lib(|l| l.upsert_track(&nt)).unwrap();

        let paired = susic_client::pair(&base, &code, "app test").unwrap();
        *ctl.server.lock().unwrap() = Some(Server::new(&base, &paired.token));
        sync_all(&ctl).unwrap();

        let all = ctl.with_lib(|l| l.list_tracks(&Default::default())).unwrap();
        assert_eq!(all.len(), 2, "shared song must not be duplicated: {all:?}");
        let shared = all.iter().find(|t| t.title == "Shared Song").unwrap();
        let remote = all.iter().find(|t| t.title == "Remote Song").unwrap();
        assert_eq!(shared.availability, "local+server");
        assert_eq!(remote.availability, "server");

        // Local copy wins over streaming; server-only track streams over HTTP.
        let p = ctl.resolve_playable(shared).unwrap();
        assert!(p.ends_with("Shared Song.wav") && !p.starts_with("http"), "{p}");
        let p = ctl.resolve_playable(remote).unwrap();
        assert!(p.starts_with(&format!("{base}/api/v1/tracks/")), "{p}");
        // …and that URL really serves the audio, seekably.
        let mut r = susic_client::HttpRangeReader::open(&p).unwrap();
        let mut head = [0u8; 4];
        std::io::Read::read_exact(&mut r, &mut head).unwrap();
        assert_eq!(&head, b"RIFF");

        // Offline download makes playback independent of the server.
        download_one(&ctl, remote.id, true).unwrap();
        let after = ctl.with_lib(|l| l.get_track(remote.id)).unwrap().unwrap();
        assert_eq!(after.availability, "downloaded");
        let p = ctl.resolve_playable(&after).unwrap();
        assert!(!p.starts_with("http") && std::path::Path::new(&p).exists());
        *ctl.server.lock().unwrap() = None; // server gone
        assert!(ctl.resolve_playable(&after).is_ok());

        // Disconnecting with "keep downloads" retains the offline song.
        let _ = std::fs::remove_dir_all(root);
    }
}
