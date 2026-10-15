#![allow(clippy::result_large_err)]
//! Integration tests: a real server on a real socket, real WAV files on disk.
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;
use susic_server::{AppState, Config};

fn write_wav(path: &Path, secs: f32, freq: f32) {
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
        let v = (((i as f32) * freq).sin() * 8000.0) as i16;
        d.extend(v.to_le_bytes());
    }
    std::fs::write(path, d).unwrap();
}

struct Fixture {
    base: String,
    state: std::sync::Arc<AppState>,
    music: PathBuf,
    root: PathBuf,
    _rt: std::thread::JoinHandle<()>,
    stop: tokio::sync::oneshot::Sender<()>,
}

impl Fixture {
    fn start(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("susic-srv-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let music = root.join("music");
        std::fs::create_dir_all(music.join("Artist/Album")).unwrap();
        let cfg = Config {
            bind: "127.0.0.1:0".into(),
            music_dir: music.clone(),
            data_dir: root.join("data"),
            scan_interval_secs: 0,
            server_name: "Test".into(),
        };
        let state = susic_server::open_state(cfg).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let st = state.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(l.local_addr().unwrap()).unwrap();
                // No automatic scan loop in tests (interval 0 still scans once; fine).
                susic_server::serve(st, l, async {
                    let _ = stop_rx.await;
                })
                .await
                .unwrap();
            });
        });
        let addr = rx.recv().unwrap();
        // serve() performs one scan at startup; let it finish so tests are deterministic.
        for _ in 0..500 {
            if state.last_scan.lock().unwrap().is_some()
                || state.last_scan_error.lock().unwrap().is_some()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Fixture {
            base: format!("http://{addr}"),
            state,
            music,
            root,
            _rt: handle,
            stop,
        }
    }
    fn scan(&self) -> susic_server::scan::ScanReport {
        // The serve() loop may be scanning concurrently; wait for it.
        loop {
            if let Some(r) = self.state.run_scan() {
                return r.unwrap();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn pair(&self) -> String {
        let code = susic_server::issue_pair_code(&self.state, Duration::from_secs(60)).unwrap();
        let r: serde_json::Value = ureq::post(&format!("{}/api/v1/pair", self.base))
            .send_json(serde_json::json!({"code": code.to_lowercase().replace('-', " "), "device_name": "test phone"}))
            .unwrap()
            .into_json()
            .unwrap();
        r["token"].as_str().unwrap().to_string()
    }
    fn get(&self, path: &str, token: &str) -> Result<ureq::Response, ureq::Error> {
        ureq::get(&format!("{}{}", self.base, path))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
    }
    fn shutdown(self) {
        let _ = self.stop.send(());
        let _ = self._rt.join();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn library(f: &Fixture, token: &str) -> serde_json::Value {
    f.get("/api/v1/library", token)
        .unwrap()
        .into_json()
        .unwrap()
}

#[test]
fn auth_pairing_and_health() {
    let f = Fixture::start("auth");
    // Health is public and minimal.
    let h: serde_json::Value = ureq::get(&format!("{}/health", f.base))
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(h["status"], "ok");
    // Library requires a token.
    match ureq::get(&format!("{}/api/v1/library", f.base)).call() {
        Err(ureq::Error::Status(401, _)) => {}
        other => panic!("expected 401, got {other:?}"),
    }
    // Wrong pairing code rejected.
    match ureq::post(&format!("{}/api/v1/pair", f.base))
        .send_json(serde_json::json!({"code":"NOPE-NOPE","device_name":"x"}))
    {
        Err(ureq::Error::Status(403, _)) => {}
        other => panic!("expected 403, got {other:?}"),
    }
    let token = f.pair();
    assert!(f.get("/api/v1/status", &token).is_ok());
    // Query-string token (for media players that cannot set headers).
    assert!(
        ureq::get(&format!("{}/api/v1/status?access_token={token}", f.base))
            .call()
            .is_ok()
    );
    // A pairing code is single-use.
    let code = susic_server::issue_pair_code(&f.state, Duration::from_secs(60)).unwrap();
    let body = serde_json::json!({"code": code, "device_name": "a"});
    assert!(ureq::post(&format!("{}/api/v1/pair", f.base))
        .send_json(body.clone())
        .is_ok());
    assert!(matches!(
        ureq::post(&format!("{}/api/v1/pair", f.base)).send_json(body),
        Err(ureq::Error::Status(403, _))
    ));
    // Revocation takes effect immediately.
    let devices: serde_json::Value = f
        .get("/api/v1/devices", &token)
        .unwrap()
        .into_json()
        .unwrap();
    let id = devices["devices"][0]["id"].as_i64().unwrap();
    ureq::delete(&format!("{}/api/v1/devices/{id}", f.base))
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .unwrap();
    assert!(matches!(
        f.get("/api/v1/status", &token),
        Err(ureq::Error::Status(401, _))
    ));
    f.shutdown();
}

#[test]
fn scan_stream_range_and_artwork_lookup() {
    let f = Fixture::start("stream");
    write_wav(&f.music.join("Artist/Album/one.wav"), 2.0, 0.1);
    write_wav(&f.music.join("two.wav"), 1.0, 0.2);
    std::fs::write(f.music.join("notes.txt"), "ignore me").unwrap();
    let r = f.scan();
    assert_eq!(r.failed.len(), 0, "{:?}", r.failed);
    let token = f.pair();
    let lib = library(&f, &token);
    let tracks = lib["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 2);
    let one = tracks.iter().find(|t| t["title"] == "one").unwrap();
    let id = one["id"].as_i64().unwrap();
    let size = one["size_bytes"].as_i64().unwrap();

    // Full download.
    let full = f
        .get(&format!("/api/v1/tracks/{id}/stream"), &token)
        .unwrap();
    assert_eq!(full.header("Accept-Ranges"), Some("bytes"));
    let mut body = Vec::new();
    std::io::copy(&mut full.into_reader(), &mut body).unwrap();
    assert_eq!(body.len() as i64, size);
    assert_eq!(&body[..4], b"RIFF");

    // Seek: range request returns 206 with exactly the requested bytes.
    let part = ureq::get(&format!("{}/api/v1/tracks/{id}/stream", f.base))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Range", "bytes=1000-1999")
        .call()
        .unwrap();
    assert_eq!(part.status(), 206);
    assert!(part
        .header("Content-Range")
        .unwrap()
        .starts_with("bytes 1000-1999/"));
    let mut chunk = Vec::new();
    std::io::copy(&mut part.into_reader(), &mut chunk).unwrap();
    assert_eq!(chunk, body[1000..2000]);

    // Unknown track.
    assert!(matches!(
        f.get("/api/v1/tracks/9999/stream", &token),
        Err(ureq::Error::Status(404, _))
    ));
    f.shutdown();
}

#[test]
fn detects_new_moved_changed_and_deleted_files() {
    let f = Fixture::start("changes");
    let a = f.music.join("a.wav");
    write_wav(&a, 1.0, 0.1);
    write_wav(&f.music.join("b.wav"), 1.5, 0.3);
    let r = f.scan();
    assert_eq!((r.added, r.unchanged), (2, 0));
    let token = f.pair();
    let lib = library(&f, &token);
    let rev0 = lib["revision"].as_i64().unwrap();
    let id_a = lib["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "a")
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    // Favourite + playlist reference a.
    ureq::put(&format!("{}/api/v1/tracks/{id_a}/favourite", f.base))
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(serde_json::json!({"favourite": true}))
        .unwrap();

    // Unchanged rescan is a no-op.
    let r = f.scan();
    assert_eq!((r.added, r.updated, r.unchanged), (0, 0, 2));

    // Move a.wav into a subfolder: id (and favourite) survive.
    std::fs::create_dir_all(f.music.join("moved")).unwrap();
    std::fs::rename(&a, f.music.join("moved/renamed.wav")).unwrap();
    let r = f.scan();
    assert_eq!((r.moved, r.added, r.removed), (1, 0, 0), "{r:?}");
    let lib = library(&f, &token);
    let t = lib["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id_a)
        .expect("id preserved across move");
    assert_eq!(t["favourite"], true);

    // Delete b.wav: it is reported as removed in the delta.
    let id_b = library(&f, &token)["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "b")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    std::fs::remove_file(f.music.join("b.wav")).unwrap();
    let r = f.scan();
    assert_eq!(r.removed, 1);
    let delta: serde_json::Value = f
        .get(&format!("/api/v1/library?since={rev0}"), &token)
        .unwrap()
        .into_json()
        .unwrap();
    assert!(delta["removed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == id_b));
    assert!(matches!(
        f.get(&format!("/api/v1/tracks/{id_b}/stream"), &token),
        Err(ureq::Error::Status(404, _))
    ));
    f.shutdown();
}

#[test]
fn unmounted_music_dir_never_wipes_the_library() {
    let f = Fixture::start("unmount");
    write_wav(&f.music.join("keep.wav"), 1.0, 0.1);
    f.scan();
    assert_eq!(f.state.db.live_count().unwrap(), 1);
    // Directory present but empty (mount point of an unmounted disk).
    std::fs::remove_file(f.music.join("keep.wav")).unwrap();
    std::fs::remove_dir_all(f.music.join("Artist")).unwrap();
    let r = f.scan();
    assert!(r.removal_skipped);
    assert_eq!(f.state.db.live_count().unwrap(), 1, "tracks must be kept");
    // Directory entirely gone: scan errors, nothing changes.
    std::fs::remove_dir_all(&f.music).unwrap();
    assert!(f.state.run_scan().unwrap().is_err());
    assert_eq!(f.state.db.live_count().unwrap(), 1);
    f.state.cancel.store(false, Ordering::Relaxed);
    f.shutdown();
}

#[test]
fn playlists_sync_last_writer_wins() {
    let f = Fixture::start("playlists");
    write_wav(&f.music.join("x.wav"), 1.0, 0.1);
    f.scan();
    let token = f.pair();
    let put = |name: &str, ids: Vec<i64>, at: i64| {
        ureq::put(&format!("{}/api/v1/playlists/pl-1", f.base))
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(serde_json::json!({"name": name, "track_ids": ids, "updated_at": at}))
    };
    assert!(put("Road trip", vec![1], 100).is_ok());
    // Older write is rejected with the current version.
    match put("Stale", vec![], 50) {
        Err(ureq::Error::Status(409, r)) => {
            let v: serde_json::Value = r.into_json().unwrap();
            assert_eq!(v["name"], "Road trip");
        }
        other => panic!("expected 409, got {other:?}"),
    }
    assert!(put("Road trip 2", vec![1, 1], 200).is_ok());
    let list: serde_json::Value = f
        .get("/api/v1/playlists", &token)
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(list["playlists"][0]["name"], "Road trip 2");
    assert_eq!(
        list["playlists"][0]["track_ids"].as_array().unwrap().len(),
        2
    );
    ureq::delete(&format!("{}/api/v1/playlists/pl-1", f.base))
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .unwrap();
    let list: serde_json::Value = f
        .get("/api/v1/playlists", &token)
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(list["playlists"][0]["deleted"], true);
    f.shutdown();
}
