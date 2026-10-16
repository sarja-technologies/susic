//! Server-side YouTube search + import over a real socket, against real
//! YouTube. Network-dependent: runs only with `SUSIC_LIVE_YTDLP=1`.
//! Test media is a Creative Commons Attribution (reuse allowed) upload.
use std::sync::Arc;
use std::time::{Duration, Instant};
use susic_client::Server;
use susic_server::{AppState, Config};

const CC_BY_PIANO: &str = "https://www.youtube.com/watch?v=XVLVeGK16so";

fn serve(state: Arc<AppState>) -> (String, tokio::sync::oneshot::Sender<()>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(l.local_addr().unwrap()).unwrap();
            susic_server::serve(state, l, async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
        });
    });
    (format!("http://{}", rx.recv().unwrap()), stop)
}

fn paired(enabled: bool, name: &str) -> (Server, Arc<AppState>, tokio::sync::oneshot::Sender<()>) {
    let root = std::env::temp_dir().join(format!("susic-yt-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("music")).unwrap();
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        music_dir: root.join("music"),
        data_dir: root.join("data"),
        scan_interval_secs: 0,
        ytdlp_enabled: enabled,
        ..Default::default()
    };
    let state = susic_server::open_state(cfg).unwrap();
    let code = susic_server::issue_pair_code(&state, Duration::from_secs(60)).unwrap();
    let (base, stop) = serve(state.clone());
    let p = susic_client::pair(&base, &code, "yt test").unwrap();
    (Server::new(&base, &p.token), state, stop)
}

#[test]
fn disabled_server_refuses_clearly() {
    let (srv, _st, _stop) = paired(false, "off");
    let caps = srv.youtube_caps().unwrap().unwrap();
    assert!(!caps.enabled);
    let e = srv
        .youtube_search("anything", false, 5)
        .unwrap_err()
        .to_string();
    assert!(e.contains("turned off"), "{e}");
    let e = srv
        .start_import(CC_BY_PIANO, "mp3", 0, "")
        .unwrap_err()
        .to_string();
    assert!(e.contains("turned off"), "{e}");
}

#[test]
fn server_installs_searches_imports_and_streams() {
    if std::env::var("SUSIC_LIVE_YTDLP").as_deref() != Ok("1") {
        return;
    }
    let (srv, st, _stop) = paired(true, "on");
    let caps = srv.youtube_caps().unwrap().unwrap();
    eprintln!("caps before install: {caps:?}");
    if !caps.ytdlp {
        // PATH has no yt-dlp here: install the managed copy through the API.
        let e = srv.youtube_search("x", false, 3).unwrap_err().to_string();
        assert!(e.contains("not installed"), "{e}");
        eprintln!("{}", srv.youtube_update().unwrap());
    }
    let caps = srv.youtube_caps().unwrap().unwrap();
    assert!(caps.can_search() && caps.can_import(), "{caps:?}");

    let t = Instant::now();
    let results = srv
        .youtube_search(
            "i think about you not thinking about me piano solo",
            false,
            8,
        )
        .unwrap();
    eprintln!("search took {:?}", t.elapsed());
    for r in &results {
        eprintln!("  {} | {} | {:?}", r.title, r.artist, r.duration_ms);
    }
    assert!(!results.is_empty());

    let job = srv.start_import(CC_BY_PIANO, "m4a", 0, "hint").unwrap();
    let mut last = String::new();
    let done = loop {
        let j = srv
            .imports()
            .unwrap()
            .into_iter()
            .find(|j| j.id == job.id)
            .unwrap();
        if j.message != last {
            eprintln!("  [{}] {:.0}% {}", j.status, j.progress * 100.0, j.message);
            last = j.message.clone();
        }
        if !matches!(j.status.as_str(), "queued" | "running") {
            break j;
        }
        assert!(job_age_ok(&job), "timed out");
        std::thread::sleep(Duration::from_millis(500));
    };
    assert_eq!(done.status, "done", "{}", done.message);
    assert!(done
        .license
        .as_deref()
        .unwrap_or("")
        .contains("Creative Commons"));
    let id = done.track_id.expect("indexed");
    let lib = srv.library(0).unwrap();
    let t = lib.tracks.iter().find(|t| t.id == id).unwrap();
    eprintln!(
        "server track: {} — {} ({} ms, {})",
        t.artist, t.title, t.duration_ms, t.format
    );
    assert_eq!(t.artist, "far");
    assert!(t.has_artwork);
    assert!(t.duration_ms > 150_000);
    assert!(st.cfg.imports_path().is_dir());
    // Stream it back exactly as clients (Android/desktop) do.
    let mut r = susic_client::HttpRangeReader::open(&srv.stream_url(id)).unwrap();
    let mut head = [0u8; 12];
    std::io::Read::read_exact(&mut r, &mut head).unwrap();
    assert_eq!(&head[4..8], b"ftyp", "m4a container");
    // Re-importing the same video reuses the existing file.
    let again = srv.start_import(CC_BY_PIANO, "m4a", 0, "").unwrap();
    loop {
        let j = srv
            .imports()
            .unwrap()
            .into_iter()
            .find(|j| j.id == again.id)
            .unwrap();
        if j.status == "done" {
            assert_eq!(j.track_id, Some(id));
            break;
        }
        assert_ne!(j.status, "failed", "{}", j.message);
        std::thread::sleep(Duration::from_millis(300));
    }
    assert_eq!(srv.library(0).unwrap().tracks.len(), 1);
}

fn job_age_ok(_j: &susic_client::RemoteImport) -> bool {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed() < Duration::from_secs(240)
}
