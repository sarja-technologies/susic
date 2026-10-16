//! YouTube search and import.
//!
//! Search uses yt-dlp's `ytsearch` (no Google account or API key) on this
//! device, or the paired Susic server's yt-dlp where this device has none
//! (Android). The official YouTube Data API remains an optional backend.
//! Imports go to this device's library or to the server's library, from
//! where every client can stream them.

use crate::commands::{Ctl, Extras};
use crate::controller::Controller;
use crate::{importer, server};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use susic_client::{YoutubeCaps, YoutubeResult};
use susic_import::ytdlp::{self, Event, SearchOptions, Tools};
use susic_library::ImportJob;
use tauri::State;

type R<T> = Result<T, String>;
type Cancels = Mutex<HashMap<i64, Arc<AtomicBool>>>;

const ANDROID: bool = cfg!(target_os = "android");

fn managed_dir(c: &Controller) -> PathBuf {
    c.data_dir.join("tools")
}

fn tools(c: &Controller) -> Tools {
    Tools::locate(Some(&managed_dir(c)))
}

fn setting(c: &Controller, key: &str) -> Option<String> {
    c.with_lib(|l| l.get_setting(key)).ok().flatten()
}

/// Server YouTube capabilities, if a server is connected and reachable.
fn server_caps(c: &Controller) -> Option<YoutubeCaps> {
    let srv = server::server_of(c).ok()?;
    srv.youtube_caps().ok().flatten()
}

// ---- status / install / update ----------------------------------------------------

#[derive(Serialize)]
pub struct YtStatus {
    /// yt-dlp can run on this device (desktop).
    supported: bool,
    ytdlp: Option<String>,
    version: Option<String>,
    managed: bool,
    ffmpeg: Option<String>,
    js_runtime: Option<String>,
    can_install: bool,
    /// The connected server's capabilities (null when none/offline/old server).
    server: Option<YoutubeCaps>,
}

#[tauri::command]
pub async fn ytdlp_status(c: Ctl<'_>) -> R<YtStatus> {
    let ctl: Arc<Controller> = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let server = server_caps(&ctl);
        if ANDROID {
            return YtStatus {
                supported: false,
                ytdlp: None,
                version: None,
                managed: false,
                ffmpeg: None,
                js_runtime: None,
                can_install: false,
                server,
            };
        }
        let s = ytdlp::tool_status(Some(&managed_dir(&ctl)));
        YtStatus {
            supported: true,
            ytdlp: s.ytdlp,
            version: s.version,
            managed: s.managed,
            ffmpeg: s.ffmpeg,
            js_runtime: s.js_runtime,
            can_install: s.can_install,
            server,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Install Susic's own yt-dlp (official release, checksum-verified).
#[tauri::command]
pub async fn ytdlp_install(c: Ctl<'_>) -> R<String> {
    if ANDROID {
        return Err(
            "yt-dlp can't run on Android. Use a Susic server with YouTube import enabled.".into(),
        );
    }
    let dir = managed_dir(&c);
    tauri::async_runtime::spawn_blocking(move || {
        ytdlp::install_managed(&dir).map(|v| format!("Installed yt-dlp {v}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Update yt-dlp on this device, or on the server (`target = "server"`).
#[tauri::command]
pub async fn ytdlp_update(c: Ctl<'_>, target: Option<String>) -> R<String> {
    let ctl: Arc<Controller> = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if target.as_deref() == Some("server") {
            return server::server_of(&ctl)?
                .youtube_update()
                .map_err(|e| e.to_string());
        }
        if ANDROID {
            return Err("yt-dlp can't run on Android.".into());
        }
        ytdlp::update(&managed_dir(&ctl))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- search -----------------------------------------------------------------------

#[derive(Serialize)]
pub struct SearchResponse {
    /// device | server | api
    source: &'static str,
    results: Vec<YoutubeResult>,
}

fn from_local(r: ytdlp::SearchResult) -> YoutubeResult {
    YoutubeResult {
        id: r.id,
        url: r.url,
        title: r.title,
        artist: r.artist,
        channel: r.channel,
        duration_ms: r.duration_ms,
        thumb: r.thumb,
        live: r.live,
    }
}

/// Search YouTube. Backend `ytdlp` (default) needs no API key: it uses this
/// device's yt-dlp, else the server's. Backend `api` uses the user's own
/// YouTube Data API key.
pub fn search_blocking(c: &Controller, query: &str, creative_commons: bool) -> R<SearchResponse> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(SearchResponse {
            source: "device",
            results: vec![],
        });
    }
    if setting(c, "youtube_backend").as_deref() == Some("api") {
        let key = setting(c, "youtube_api_key")
            .filter(|k| !k.trim().is_empty())
            .ok_or(
                "Add your YouTube Data API key in Settings, or switch the search backend to yt-dlp",
            )?;
        return Ok(SearchResponse {
            source: "api",
            results: api_search(query, key.trim())?,
        });
    }
    let opts = SearchOptions {
        limit: 15,
        creative_commons,
    };
    if !ANDROID {
        let t = tools(c);
        if t.ytdlp.is_some() {
            let results = ytdlp::search(&t, query, &opts)?;
            return Ok(SearchResponse {
                source: "device",
                results: results.into_iter().map(from_local).collect(),
            });
        }
    }
    match server::server_of(c) {
        Ok(srv) => {
            let r = srv
                .youtube_search(query, creative_commons, opts.limit)
                .map_err(|e| match e {
                    susic_client::Error::Server(m) if m.contains("not found") || m.contains("HTTP 404") => {
                        "Your Susic server is too old for YouTube search — update it to 0.3.".to_string()
                    }
                    e => e.to_string(),
                })?;
            Ok(SearchResponse { source: "server", results: r })
        }
        Err(_) if ANDROID => Err(
            "YouTube search on Android runs through your Susic server. Connect a server with YouTube import enabled."
                .into(),
        ),
        Err(_) => Err("yt-dlp isn't installed. Install it in Settings → YouTube (one click).".into()),
    }
}

#[tauri::command]
pub async fn youtube_search(
    c: Ctl<'_>,
    query: String,
    creative_commons: Option<bool>,
) -> R<SearchResponse> {
    let ctl: Arc<Controller> = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        search_blocking(&ctl, &query, creative_commons.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Optional official API backend (100 quota units per search).
fn api_search(query: &str, key: &str) -> R<Vec<YoutubeResult>> {
    let resp = ureq::get("https://www.googleapis.com/youtube/v3/search")
        .query("part", "snippet")
        .query("type", "video")
        .query("videoCategoryId", "10")
        .query("maxResults", "15")
        .query("q", query)
        .query("key", key)
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
    let decode = |s: &str| {
        s.replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
    };
    Ok(v["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    let id = i["id"]["videoId"].as_str()?.to_string();
                    let channel = decode(i["snippet"]["channelTitle"].as_str().unwrap_or(""));
                    Some(YoutubeResult {
                        url: format!("https://www.youtube.com/watch?v={id}"),
                        title: decode(i["snippet"]["title"].as_str()?),
                        artist: channel.trim_end_matches(" - Topic").to_string(),
                        channel,
                        duration_ms: None,
                        thumb: i["snippet"]["thumbnails"]["medium"]["url"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        live: i["snippet"]["liveBroadcastContent"].as_str() == Some("live"),
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

// ---- import -------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct UrlImport {
    pub url: String,
    pub format: String,
    pub quality: u32,
    pub dest_dir: Option<String>,
    /// device (default) | server
    pub target: Option<String>,
    /// Display hint from the search result.
    pub title: Option<String>,
    /// Thumbnail URL, used as cover art if embedding fails.
    pub thumb: Option<String>,
}

/// Device imports run one at a time (gentle on the source, predictable on disk).
static DEVICE_QUEUE: Mutex<()> = Mutex::new(());

fn set_job(c: &Controller, id: i64, status: &str, p: f64, msg: &str) {
    let _ = c.with_lib(|l| l.update_import_job(id, status, p, msg));
    c.emit("jobs://changed", json!({ "id": id }));
}

pub fn start_import(c: &Arc<Controller>, cancels: &Cancels, req: UrlImport) -> R<i64> {
    let target = req.target.clone().unwrap_or_else(|| "device".into());
    ytdlp::validate_url(&req.url)?;
    if !ytdlp::FORMATS.contains(&req.format.as_str()) {
        return Err(format!("Unsupported format '{}'", req.format));
    }
    match target.as_str() {
        "device" if ANDROID => {
            return Err(
                "Importing to this phone isn't possible on Android (yt-dlp can't run here). Import to your server instead."
                    .into(),
            )
        }
        "device" => {
            let t = tools(c);
            if t.ytdlp.is_none() {
                return Err("yt-dlp isn't installed. Install it in Settings → YouTube.".into());
            }
            if t.ffmpeg.is_none() {
                return Err("FFmpeg isn't installed. It's needed to extract audio — see Settings → YouTube.".into());
            }
        }
        "server" => {
            server::server_of(c)?;
        }
        other => return Err(format!("Unknown import target '{other}'")),
    }
    let url = req.url.trim().to_string();
    let dup = c.with_lib(|l| l.import_jobs())?.iter().any(|j| {
        j.url == url && j.target == target && (j.status == "queued" || j.status == "running")
    });
    if dup {
        return Err("This is already in the import queue".into());
    }
    let title = req.title.clone().unwrap_or_default();
    let id = c.with_lib(|l| l.add_import_job(&url, &title, &target))?;
    let cancel = Arc::new(AtomicBool::new(false));
    cancels.lock().unwrap().insert(id, cancel.clone());
    let ctl = c.clone();
    std::thread::spawn(move || {
        if target == "server" {
            run_server_job(&ctl, id, &req, &cancel)
        } else {
            run_device_job(&ctl, id, &req, &cancel)
        }
    });
    c.emit("jobs://changed", json!({ "id": id }));
    Ok(id)
}

#[tauri::command]
pub fn import_url(c: Ctl, ex: State<Extras>, req: UrlImport) -> R<i64> {
    start_import(c.inner(), &ex.job_cancels, req)
}

fn run_device_job(c: &Arc<Controller>, id: i64, req: &UrlImport, cancel: &Arc<AtomicBool>) {
    set_job(c, id, "queued", 0.0, "Waiting for other imports…");
    let _turn = DEVICE_QUEUE.lock().unwrap_or_else(|p| p.into_inner());
    if cancel.load(Ordering::SeqCst) {
        return set_job(c, id, "cancelled", 0.0, "Cancelled");
    }
    set_job(c, id, "running", 0.0, "Starting…");
    let dest = req
        .dest_dir
        .clone()
        .filter(|d| !d.trim().is_empty())
        .or_else(|| setting(c, "import_dir").filter(|d| !d.trim().is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| c.music_dir.clone());
    let opts = ytdlp::Options {
        dest_dir: dest,
        format: req.format.clone(),
        quality: req.quality,
    };
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut thumb = req.thumb.clone();
    let res = ytdlp::run(
        &tools(c),
        &req.url,
        &opts,
        cancel.clone(),
        &mut |ev| match ev {
            Event::Progress(p, msg) => {
                if last.elapsed().as_millis() > 250 {
                    last = Instant::now();
                    set_job(c, id, "running", p, &msg);
                }
            }
            Event::Info(i) => {
                if thumb.is_none() {
                    thumb = i.thumbnail.clone();
                }
                let _ = c.with_lib(|l| {
                    l.set_import_job_result(
                        id,
                        Some(&i.title).filter(|t| !t.is_empty()).map(String::as_str),
                        i.license.as_deref().filter(|l| !l.is_empty()),
                        None,
                    )
                });
            }
            Event::File(_) => {}
        },
    );
    let d = match res {
        Ok(d) => d,
        Err(e) if e == "cancelled" => return set_job(c, id, "cancelled", 0.0, "Cancelled"),
        Err(e) => return set_job(c, id, "failed", 0.0, &e),
    };
    set_job(c, id, "running", 0.97, "Adding to your library…");
    let sum = importer::run_import(c, d.files.clone(), "yt-dlp", true);
    // run_import reports file-import progress; clear that banner (this job has its own status).
    c.emit("import://idle", json!({}));
    let path = d.files[0].to_string_lossy().into_owned();
    let track_id = c
        .with_lib(|l| l.known_path(&path))
        .ok()
        .flatten()
        .map(|(tid, _)| tid);
    if let Some(tid) = track_id {
        ensure_artwork(c, tid, thumb.as_deref());
        let _ = c.with_lib(|l| l.set_import_job_result(id, None, None, Some(tid)));
    }
    c.emit("library://changed", json!({ "reason": "import" }));
    if track_id.is_none() && !sum.failed.is_empty() {
        return set_job(c, id, "failed", 1.0, &sum.failed[0]);
    }
    let msg = if d.reused || (sum.added + sum.updated == 0 && sum.duplicates > 0) {
        "Already in your library"
    } else {
        "Added to your library"
    };
    set_job(c, id, "done", 1.0, msg);
    c.emit("import://notify", json!({ "message": msg }));
}

/// Fall back to the video thumbnail when yt-dlp couldn't embed artwork
/// (e.g. a system yt-dlp without mutagen for Opus files).
fn ensure_artwork(c: &Controller, track_id: i64, thumb: Option<&str>) {
    let has = c
        .with_lib(|l| l.get_track(track_id))
        .ok()
        .flatten()
        .map(|t| t.artwork.is_some())
        .unwrap_or(true);
    if has {
        return;
    }
    if let Some(art) = thumb
        .and_then(ytdlp::fetch_image)
        .and_then(|b| susic_import::save_thumbnail(&b, &c.artwork_dir))
    {
        let _ = c.with_lib(|l| l.set_artwork(track_id, &art));
    }
}

fn run_server_job(c: &Arc<Controller>, id: i64, req: &UrlImport, cancel: &Arc<AtomicBool>) {
    set_job(c, id, "running", 0.0, "Sending to your server…");
    let srv = match server::server_of(c) {
        Ok(s) => s,
        Err(e) => return set_job(c, id, "failed", 0.0, &e),
    };
    let remote = match srv.start_import(
        &req.url,
        &req.format,
        req.quality,
        req.title.as_deref().unwrap_or(""),
    ) {
        Ok(j) => j,
        Err(e) => return set_job(c, id, "failed", 0.0, &e.to_string()),
    };
    let mut unreachable_since: Option<Instant> = None;
    let mut last_msg = String::new();
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = srv.cancel_import(remote.id);
            return set_job(c, id, "cancelled", 0.0, "Cancelled");
        }
        match srv.imports() {
            Ok(jobs) => {
                unreachable_since = None;
                let Some(j) = jobs.into_iter().find(|j| j.id == remote.id) else {
                    return set_job(
                        c,
                        id,
                        "failed",
                        0.0,
                        "The server restarted and lost this import — try again.",
                    );
                };
                if j.message != last_msg || j.status != "running" {
                    last_msg = j.message.clone();
                    let _ = c.with_lib(|l| {
                        l.set_import_job_result(
                            id,
                            Some(j.title.as_str()).filter(|t| !t.is_empty()),
                            j.license.as_deref(),
                            None,
                        )
                    });
                }
                match j.status.as_str() {
                    "queued" => set_job(c, id, "running", 0.0, "Queued on your server…"),
                    "running" => set_job(
                        c,
                        id,
                        "running",
                        j.progress,
                        &format!("Server: {}", j.message),
                    ),
                    "done" => {
                        set_job(c, id, "running", 0.98, "Syncing with your server…");
                        let _ = server::sync_all(c);
                        let local = j
                            .track_id
                            .and_then(|rid| c.with_lib(|l| l.track_by_remote(rid)).ok().flatten());
                        let _ = c.with_lib(|l| l.set_import_job_result(id, None, None, local));
                        let msg = "Added to your server — playable on all your devices";
                        set_job(c, id, "done", 1.0, msg);
                        c.emit("import://notify", json!({ "message": msg }));
                        return;
                    }
                    "cancelled" => {
                        return set_job(c, id, "cancelled", 0.0, "Cancelled on the server")
                    }
                    _ => return set_job(c, id, "failed", 0.0, &j.message),
                }
            }
            Err(e) if e.is_transient() => {
                let since = *unreachable_since.get_or_insert_with(Instant::now);
                if since.elapsed() > Duration::from_secs(600) {
                    return set_job(c, id, "failed", 0.0, "Lost contact with your server.");
                }
                set_job(
                    c,
                    id,
                    "running",
                    0.0,
                    "Waiting for your server to come back…",
                );
            }
            Err(e) => return set_job(c, id, "failed", 0.0, &e.to_string()),
        }
        std::thread::sleep(Duration::from_millis(800));
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

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    //! Live end-to-end checks against real YouTube; run with
    //! `SUSIC_LIVE_YTDLP=1 cargo test -p susic-app youtube -- --nocapture`.
    //! Test media: Creative Commons Attribution uploads only.
    use super::*;
    use crate::engine::{Engine, MediaInfo};
    use rodio::Source;
    use std::sync::Mutex as StdMutex;

    const CC_BY_PIANO: &str = "https://www.youtube.com/watch?v=XVLVeGK16so";
    const CC_BY_FILM: &str = "https://www.youtube.com/watch?v=aqz-KE-bpKQ";

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

    /// Decode 15 s through the desktop engine's real decoder path.
    fn decodes(media: &MediaInfo) -> usize {
        let dec = crate::engine::desktop::open(media).expect("decoder opens");
        let rate = dec.sample_rate().get() as usize * dec.channels().get() as usize;
        let samples: Vec<f32> = dec.take(rate * 15).collect(); // the test track opens with ~3 s of silence
        assert!(
            samples.iter().any(|s| s.abs() > 0.01),
            "audio is not silent"
        );
        samples.len()
    }

    fn wait_job(c: &Controller, id: i64) -> ImportJob {
        let start = Instant::now();
        let mut last = String::new();
        loop {
            let j = c
                .with_lib(|l| l.import_jobs())
                .unwrap()
                .into_iter()
                .find(|j| j.id == id)
                .unwrap();
            if j.message != last {
                eprintln!(
                    "  #{id} [{}] {:.0}% {}",
                    j.status,
                    j.progress * 100.0,
                    j.message
                );
                last = j.message.clone();
            }
            if !matches!(j.status.as_str(), "queued" | "running") {
                return j;
            }
            assert!(start.elapsed() < Duration::from_secs(300), "timed out");
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn req(url: &str, format: &str, target: &str) -> UrlImport {
        UrlImport {
            url: url.into(),
            format: format.into(),
            quality: 192,
            dest_dir: None,
            target: Some(target.into()),
            title: None,
            thumb: None,
        }
    }

    #[test]
    fn search_import_play_on_device_and_via_server() {
        if std::env::var("SUSIC_LIVE_YTDLP").as_deref() != Ok("1") {
            return;
        }
        let root = std::env::temp_dir().join(format!("susic-app-yt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let emit: crate::controller::Emitter = Arc::new(|_, _| {});
        let ctl = Controller::new(root.join("app"), emit).unwrap();
        let rec = Arc::new(Rec(StdMutex::new(vec![])));
        ctl.set_engine(Box::new(rec.clone()));
        let cancels: Cancels = Default::default();

        // Not installed yet (nothing on PATH here): clear guidance, no crash.
        if tools(&ctl).ytdlp.is_none() {
            let e = search_blocking(&ctl, "piano", false).err().unwrap();
            assert!(e.contains("Install it in Settings"), "{e}");
            eprintln!("{}", ytdlp::install_managed(&managed_dir(&ctl)).unwrap());
        }

        // 1. Real search, no API key.
        let r = search_blocking(
            &ctl,
            "far i think about you not thinking about me piano",
            false,
        )
        .unwrap();
        assert_eq!(r.source, "device");
        for x in &r.results {
            eprintln!("  {} — {} ({:?})", x.artist, x.title, x.duration_ms);
        }
        let hit = r
            .results
            .iter()
            .find(|x| x.url == CC_BY_PIANO)
            .expect("CC BY test track in results");

        // 2. Import the chosen result to this device, then play it.
        let mut q = req(&hit.url, "m4a", "device");
        q.title = Some(hit.title.clone());
        q.thumb = Some(hit.thumb.clone());
        let id = start_import(&ctl, &cancels, q).unwrap();
        let job = wait_job(&ctl, id);
        assert_eq!(job.status, "done", "{}", job.message);
        assert!(job.license.contains("Creative Commons"), "{}", job.license);
        let tid = job.track_id.expect("track id");
        let t = ctl.with_lib(|l| l.get_track(tid)).unwrap().unwrap();
        eprintln!(
            "  library: {} — {} [{} ms, {}, art={:?}]",
            t.artist, t.title, t.duration_ms, t.format, t.artwork
        );
        assert_eq!(
            (t.artist.as_str(), t.format.as_str(), t.source.as_str()),
            ("far", "m4a", "yt-dlp")
        );
        assert!(t.artwork.is_some());
        assert!(t.path.starts_with(&*ctl.music_dir.to_string_lossy()));
        ctl.play_tracks(vec![tid], 0, None);
        let loaded = rec
            .0
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("engine got a load");
        assert_eq!(loaded.path, t.path);
        eprintln!("  decoded {} samples from the local file", decodes(&loaded));

        // Duplicate guard and reuse.
        let id2 = start_import(&ctl, &cancels, req(CC_BY_PIANO, "m4a", "device")).unwrap();
        assert_eq!(wait_job(&ctl, id2).message, "Already in your library");
        assert_eq!(ctl.with_lib(|l| l.stats()).unwrap().tracks, 1);

        // 3. Cancel a long import promptly.
        let id3 = start_import(&ctl, &cancels, req(CC_BY_FILM, "flac", "device")).unwrap();
        while ctl
            .with_lib(|l| l.import_jobs())
            .unwrap()
            .iter()
            .find(|j| j.id == id3)
            .unwrap()
            .progress
            <= 0.0
        {
            std::thread::sleep(Duration::from_millis(100));
        }
        cancels.lock().unwrap()[&id3].store(true, Ordering::SeqCst);
        assert_eq!(wait_job(&ctl, id3).status, "cancelled");
        let leftovers: Vec<_> = std::fs::read_dir(&ctl.music_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("aqz-KE-bpKQ"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        // 4. Server import: a real server with yt-dlp enabled; the track
        //    syncs back and streams like any server track (Android path).
        let music = root.join("srv-music");
        std::fs::create_dir_all(&music).unwrap();
        let state = susic_server::open_state(susic_server::Config {
            bind: "127.0.0.1:0".into(),
            music_dir: music,
            data_dir: root.join("srv"),
            scan_interval_secs: 0,
            ytdlp_enabled: true,
            ..Default::default()
        })
        .unwrap();
        // Share the already-verified managed yt-dlp with the server.
        std::fs::create_dir_all(state.cfg.tools_dir()).unwrap();
        std::fs::copy(
            ytdlp::managed_path(&managed_dir(&ctl)),
            ytdlp::managed_path(&state.cfg.tools_dir()),
        )
        .unwrap();
        let code = susic_server::issue_pair_code(&state, Duration::from_secs(60)).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let st = state.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(l.local_addr().unwrap()).unwrap();
                susic_server::serve(st, l, std::future::pending())
                    .await
                    .unwrap();
            });
        });
        let base = format!("http://{}", rx.recv().unwrap());
        let paired = susic_client::pair(&base, &code, "app test").unwrap();
        *ctl.server.lock().unwrap() = Some(susic_client::Server::new(&base, &paired.token));

        let id4 = start_import(&ctl, &cancels, req(CC_BY_PIANO, "mp3", "server")).unwrap();
        let job = wait_job(&ctl, id4);
        assert_eq!(job.status, "done", "{}", job.message);
        let tid = job.track_id.expect("synced local id");
        let t = ctl.with_lib(|l| l.get_track(tid)).unwrap().unwrap();
        eprintln!(
            "  server track in app: {} — {} [{}], {}",
            t.artist, t.title, t.format, t.availability
        );
        assert_eq!(t.availability, "server");
        assert_eq!(t.format, "mp3");
        let url = ctl.resolve_playable(&t).unwrap();
        assert!(url.starts_with(&base), "{url}");
        let media = MediaInfo {
            path: url,
            format: t.format.clone(),
            ..Default::default()
        };
        eprintln!(
            "  decoded {} samples streamed from the server",
            decodes(&media)
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
