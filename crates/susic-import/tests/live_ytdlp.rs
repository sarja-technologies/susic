//! Live end-to-end checks against real YouTube via yt-dlp. Network-dependent,
//! so they only run with `SUSIC_LIVE_YTDLP=1`:
//!
//!     SUSIC_LIVE_YTDLP=1 cargo test -p susic-import --test live_ytdlp -- --nocapture --test-threads=1
//!
//! Test media is limited to items YouTube reports as Creative Commons
//! Attribution licensed (reuse allowed).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use susic_import::ytdlp::{self, Event, Options, SearchOptions, Tools};

/// "Big Buck Bunny" — official Blender Foundation upload, CC BY.
const CC_BY_FILM: &str = "https://www.youtube.com/watch?v=aqz-KE-bpKQ";
/// Short CC BY piano piece (licence verified via yt-dlp's `license` field).
const CC_BY_PIANO: &str = "https://www.youtube.com/watch?v=XVLVeGK16so";

fn live() -> bool {
    std::env::var("SUSIC_LIVE_YTDLP").as_deref() == Ok("1")
}

fn workdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("susic-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Uses Susic's own managed install (download + checksum verification).
fn tools() -> Tools {
    let managed = std::env::temp_dir().join("susic-live-tools");
    if !ytdlp::managed_path(&managed).is_file() {
        let v = ytdlp::install_managed(&managed).expect("install yt-dlp");
        eprintln!("installed managed yt-dlp {v}");
    }
    let t = Tools::locate(Some(&managed));
    assert!(t.managed);
    t
}

#[test]
fn search_returns_real_results() {
    if !live() {
        return;
    }
    let t = tools();
    let r = ytdlp::search(&t, "big buck bunny blender", &SearchOptions::default()).unwrap();
    for x in &r {
        eprintln!(
            "{} | {} | {:?} | {}",
            x.title, x.artist, x.duration_ms, x.url
        );
    }
    assert!(r.len() >= 5, "got {} results", r.len());
    assert!(r
        .iter()
        .all(|x| x.url.starts_with("https://") && !x.title.is_empty()));
    assert!(r.iter().any(|x| x.duration_ms.unwrap_or(0) > 0));
    assert!(r.iter().all(|x| x.thumb.starts_with("https://")));

    let cc = ytdlp::search(
        &t,
        "piano",
        &SearchOptions {
            limit: 5,
            creative_commons: true,
        },
    )
    .unwrap();
    eprintln!(
        "CC-only: {:?}",
        cc.iter().map(|x| &x.title).collect::<Vec<_>>()
    );
    assert!(!cc.is_empty());
}

#[test]
fn imports_cc_licensed_audio_with_metadata_and_artwork() {
    if !live() {
        return;
    }
    let t = tools();
    let dir = workdir("import");
    let o = Options {
        dest_dir: dir.clone(),
        format: "mp3".into(),
        quality: 192,
        ..Default::default()
    };
    let mut progress = Vec::new();
    let mut info = None;
    let d = ytdlp::run(
        &t,
        CC_BY_PIANO,
        &o,
        Arc::new(AtomicBool::new(false)),
        &mut |e| match e {
            Event::Progress(f, m) => progress.push((f, m)),
            Event::Info(i) => info = Some(i),
            Event::File(_) => {}
        },
    )
    .unwrap();
    assert!(!d.reused);
    let info = info.expect("media info");
    eprintln!("licence: {:?}", info.license);
    assert!(info
        .license
        .unwrap_or_default()
        .contains("Creative Commons"));
    assert!(
        progress.iter().any(|(f, _)| *f > 0.5),
        "progress events: {}",
        progress.len()
    );
    assert_eq!(d.files.len(), 1);
    let f = &d.files[0];
    assert_eq!(f.extension().unwrap(), "mp3");
    // No stray partial/intermediate files.
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(left, vec![f.clone()], "leftovers: {left:?}");

    let track = susic_import::read_track(f, &dir.join("art"), "yt-dlp").unwrap();
    eprintln!(
        "{} — {} [{} ms] art={:?}",
        track.artist, track.title, track.duration_ms, track.artwork
    );
    assert!(track.title.contains("i think about you"));
    assert_eq!(track.artist, "far");
    assert!(track.duration_ms > 150_000);
    let art = track.artwork.expect("embedded artwork");
    let img = image::open(&art).unwrap();
    assert_eq!(img.width(), img.height(), "cover is square-cropped");

    // Second import of the same id reuses the file instead of downloading.
    let again = ytdlp::run(
        &t,
        CC_BY_PIANO,
        &o,
        Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    assert!(again.reused);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn cancel_stops_download_and_cleans_up() {
    if !live() {
        return;
    }
    let t = tools();
    let dir = workdir("cancel");
    let o = Options {
        dest_dir: dir.clone(),
        format: "flac".into(),
        quality: 0,
        ..Default::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    let start = std::time::Instant::now();
    let r = ytdlp::run(&t, CC_BY_FILM, &o, cancel, &mut |e| {
        if let Event::Progress(f, _) = e {
            if f > 0.0 {
                c2.store(true, Ordering::SeqCst);
            }
        }
    });
    assert_eq!(r.unwrap_err(), "cancelled");
    eprintln!("cancelled after {:?}", start.elapsed());
    std::thread::sleep(std::time::Duration::from_millis(500));
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(left.is_empty(), "leftovers after cancel: {left:?}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn unavailable_video_gives_clear_error() {
    if !live() {
        return;
    }
    let t = tools();
    let dir = workdir("err");
    let o = Options {
        dest_dir: dir.clone(),
        format: "mp3".into(),
        quality: 0,
        ..Default::default()
    };
    let e = ytdlp::run(
        &t,
        "https://www.youtube.com/watch?v=BaW_jenozKc",
        &o,
        Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap_err();
    eprintln!("error: {e}");
    assert!(e.contains("unavailable"), "{e}");
    std::fs::remove_dir_all(dir).ok();
}
