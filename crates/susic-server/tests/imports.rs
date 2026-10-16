#![cfg(unix)]
//! Server-side import security and limits over a real socket, using a fake
//! yt-dlp (offline and deterministic, so this runs in CI).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use susic_server::Config;

const FAKE: &str = r#"#!/bin/bash
here="$(cd "$(dirname "$0")" && pwd)"
if [ "$1" = "--version" ]; then echo 2026.01.01; exit 0; fi
last="${@: -1}"
case "$last" in
  ytsearch*slow*) sleep 3; echo '{"entries":[]}'; exit 0 ;;
  ytsearch*) echo '{"entries":[{"id":"abcdefghijk","ie_key":"Youtube","title":"T","channel":"C","duration":3}]}'; exit 0 ;;
esac
out=""; prev=""
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
dest="$(dirname "$out")"
id="$(echo "$last" | sed -E 's/.*v=([A-Za-z0-9_-]{11}).*/\1/')"
echo "SUSIC_INFO {\"id\":\"$id\",\"title\":\"Fake $id\",\"license\":\"Creative Commons Attribution license (reuse allowed)\"}"
case "$last" in
  *slow*)
    sleep 300 &
    echo $! >> "$here/grandchildren"
    echo "SUSIC_PROGRESS 1024 4096 100 3"
    wait ;;
  *)
    cp "$here/fixture.wav" "$dest/Fake - T [$id].wav"
    echo "SUSIC_FILE $dest/Fake - T [$id].wav" ;;
esac
"#;

fn wav(path: &Path) {
    let (rate, n) = (8000u32, 16000u32);
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
        d.extend((((i as f32 * 0.09).sin() * 7000.0) as i16).to_le_bytes());
    }
    std::fs::write(path, d).unwrap();
}

struct Srv {
    base: String,
    token: String,
    root: PathBuf,
}

impl Srv {
    fn start(name: &str, tweak: impl FnOnce(&mut Config)) -> Self {
        let root = std::env::temp_dir().join(format!("susic-imp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tools = root.join("data/tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::create_dir_all(root.join("music")).unwrap();
        for (n, body) in [("yt-dlp", FAKE), ("ffmpeg", "#!/bin/sh\nexit 0\n")] {
            std::fs::write(tools.join(n), body).unwrap();
            std::fs::set_permissions(tools.join(n), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        wav(&tools.join("fixture.wav"));
        let mut cfg = Config {
            bind: "127.0.0.1:0".into(),
            music_dir: root.join("music"),
            data_dir: root.join("data"),
            scan_interval_secs: 0,
            ytdlp_enabled: true,
            imports_min_free_mb: 0,
            ..Default::default()
        };
        tweak(&mut cfg);
        let state = susic_server::open_state(cfg).unwrap();
        let code = susic_server::issue_pair_code(&state, Duration::from_secs(60)).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .max_blocking_threads(8)
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(l.local_addr().unwrap()).unwrap();
                susic_server::serve(state, l, std::future::pending())
                    .await
                    .unwrap();
            });
        });
        let base = format!("http://{}", rx.recv().unwrap());
        let v: serde_json::Value = ureq::post(&format!("{base}/api/v1/pair"))
            .send_json(serde_json::json!({ "code": code, "device_name": "t" }))
            .unwrap()
            .into_json()
            .unwrap();
        Srv {
            base,
            token: v["token"].as_str().unwrap().into(),
            root,
        }
    }
    fn req(&self, method: &str, path: &str) -> ureq::Request {
        ureq::request(method, &format!("{}/api/v1{path}", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
    }
    /// (status, body)
    fn import(&self, url: &str) -> (u16, serde_json::Value) {
        match self
            .req("POST", "/imports")
            .send_json(serde_json::json!({ "url": url, "format": "mp3" }))
        {
            Ok(r) => (r.status(), r.into_json().unwrap()),
            Err(ureq::Error::Status(c, r)) => (c, r.into_json().unwrap()),
            Err(e) => panic!("{e}"),
        }
    }
    fn job(&self, id: u64) -> serde_json::Value {
        let v: serde_json::Value = self
            .req("GET", "/imports")
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        v["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == id)
            .cloned()
            .unwrap()
    }
    fn wait_status(&self, id: u64, want: &str) -> serde_json::Value {
        let t0 = Instant::now();
        loop {
            let j = self.job(id);
            if j["status"] == want {
                return j;
            }
            assert!(t0.elapsed() < Duration::from_secs(20), "job {id}: {j}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn alive(pid: i32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|s| !s.contains(") Z "))
        .unwrap_or(false)
}

#[test]
fn new_routes_require_authentication() {
    let s = Srv::start("auth", |_| {});
    for (m, p) in [
        ("GET", "/youtube"),
        ("GET", "/youtube/search?q=x"),
        ("POST", "/youtube/update"),
        ("GET", "/imports"),
        ("POST", "/imports"),
        ("DELETE", "/imports/1"),
    ] {
        let url = format!("{}/api/v1{p}", s.base);
        for auth in [None, Some("Bearer not-a-real-token")] {
            let mut r = ureq::request(m, &url);
            if let Some(a) = auth {
                r = r.set("Authorization", a);
            }
            match r.call() {
                Err(ureq::Error::Status(401, _)) => {}
                other => panic!("{m} {p} with {auth:?}: {:?}", other.map(|r| r.status())),
            }
        }
    }
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn only_allowed_hosts_and_clean_urls_are_imported() {
    let s = Srv::start("hosts", |_| {});
    for (url, why) in [
        ("http://192.168.1.1/admin.mp3", "only imports from"),
        ("https://example.com/song", "only imports from"),
        (
            "https://evil.example/?u=https://youtube.com/watch?v=abcdefghijk",
            "only imports from",
        ),
        (
            "https://youtube.com.evil.example/watch?v=abcdefghijk",
            "only imports from",
        ),
        (
            "https://user:pw@www.youtube.com/watch?v=abcdefghijk",
            "credentials",
        ),
        ("file:///etc/passwd", "valid http"),
        ("https://www.youtube.com/playlist?list=PL1", "Playlist"),
    ] {
        let (code, body) = s.import(url);
        assert_eq!(code, 400, "{url}: {body}");
        assert!(
            body["error"].as_str().unwrap().contains(why),
            "{url}: {body}"
        );
    }
    let long = format!(
        "https://www.youtube.com/watch?v=abcdefghijk&x={}",
        "a".repeat(3000)
    );
    assert_eq!(s.import(&long).0, 400);
    // Subdomains of allowed hosts are fine.
    let (code, body) = s.import("https://music.youtube.com/watch?v=okokokokok1");
    assert_eq!(code, 202, "{body}");
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn successful_import_is_indexed_and_streamable() {
    let s = Srv::start("ok", |_| {});
    let (code, job) = s.import("https://www.youtube.com/watch?v=okokokokok1");
    assert_eq!(code, 202, "{job}");
    let done = s.wait_status(job["id"].as_u64().unwrap(), "done");
    assert!(done["license"]
        .as_str()
        .unwrap()
        .contains("Creative Commons"));
    let tid = done["track_id"].as_i64().expect("indexed");
    let r = s
        .req("GET", &format!("/tracks/{tid}/stream"))
        .call()
        .unwrap();
    assert_eq!(r.header("Content-Type"), Some("audio/wav"));
    assert!(s.root.join("music/Susic Imports").is_dir());
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn queue_is_bounded_and_cancel_kills_the_process_tree() {
    let s = Srv::start("queue", |c| c.imports_max_queue = 2);
    let (c1, j1) = s.import("https://www.youtube.com/watch?v=slowslowsl1");
    let (c2, j2) = s.import("https://www.youtube.com/watch?v=slowslowsl2");
    assert_eq!((c1, c2), (202, 202));
    // Same URL again returns the existing job instead of queueing a duplicate.
    assert_eq!(
        s.import("https://www.youtube.com/watch?v=slowslowsl1").1["id"],
        j1["id"]
    );
    let (c3, b3) = s.import("https://www.youtube.com/watch?v=slowslowsl3");
    assert_eq!(c3, 429, "{b3}");
    assert!(b3["error"].as_str().unwrap().contains("queue is full"));

    let (id1, id2) = (j1["id"].as_u64().unwrap(), j2["id"].as_u64().unwrap());
    s.wait_status(id1, "running");
    let pids = s.root.join("data/tools/grandchildren");
    let t0 = Instant::now();
    while !pids.exists() {
        assert!(t0.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(50));
    }
    let gc: i32 = std::fs::read_to_string(&pids)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(alive(gc));
    // Cancel the queued one first, then the running one.
    s.req("DELETE", &format!("/imports/{id2}")).call().unwrap();
    s.req("DELETE", &format!("/imports/{id1}")).call().unwrap();
    s.wait_status(id1, "cancelled");
    s.wait_status(id2, "cancelled");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!alive(gc), "FFmpeg stand-in survived cancel");
    // Room in the queue again.
    assert_eq!(
        s.import("https://www.youtube.com/watch?v=okokokokok1").0,
        202
    );
    let left: Vec<_> = std::fs::read_dir(s.root.join("music/Susic Imports"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .filter(|n| n.to_string_lossy().contains("slow"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn low_disk_space_is_refused_with_507() {
    let s = Srv::start("disk", |c| c.imports_min_free_mb = u64::MAX / (1 << 20));
    let (code, body) = s.import("https://www.youtube.com/watch?v=okokokokok1");
    assert_eq!(code, 507, "{body}");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("low on disk space"));
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn searches_are_bounded_and_never_starve_other_requests() {
    let s = Srv::start("search", |_| {});
    let fire = || {
        let r = s.req("GET", "/youtube/search").query("q", "slow");
        std::thread::spawn(move || match r.call() {
            Ok(r) => r.status(),
            Err(ureq::Error::Status(c, _)) => c,
            Err(e) => panic!("{e}"),
        })
    };
    let hs: Vec<_> = (0..6).map(|_| fire()).collect();
    std::thread::sleep(Duration::from_millis(400));
    // While searches run, library/status requests still answer promptly.
    for _ in 0..5 {
        let t0 = Instant::now();
        s.req("GET", "/status").call().unwrap();
        s.req("GET", "/library?since=0").call().unwrap();
        assert!(t0.elapsed() < Duration::from_secs(1), "{:?}", t0.elapsed());
    }
    let codes: Vec<u16> = hs.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(codes.iter().filter(|c| **c == 200).count(), 2, "{codes:?}");
    assert_eq!(codes.iter().filter(|c| **c == 429).count(), 4, "{codes:?}");
    // Slots are released afterwards.
    let r = s
        .req("GET", "/youtube/search")
        .query("q", "x")
        .call()
        .unwrap();
    let v: serde_json::Value = r.into_json().unwrap();
    assert_eq!(v["results"][0]["id"], "abcdefghijk");
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn unwritable_imports_folder_is_reported_up_front() {
    let s = Srv::start("rofs", |_| {});
    let dir = s.root.join("music/Susic Imports");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(dir.join("probe"), b"").is_ok() {
        // Running as root: permissions don't apply, nothing to test.
        std::fs::remove_file(dir.join("probe")).ok();
        return;
    }
    let caps: serde_json::Value = s
        .req("GET", "/youtube")
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(caps["imports_writable"], false, "{caps}");
    let (code, body) = s.import("https://www.youtube.com/watch?v=okokokokok1");
    assert_eq!(code, 403, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("can't write"),
        "{body}"
    );
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let caps: serde_json::Value = s
        .req("GET", "/youtube")
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(caps["imports_writable"], true);
    std::fs::remove_dir_all(&s.root).ok();
}
