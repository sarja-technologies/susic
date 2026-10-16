//! Offline, deterministic checks of the yt-dlp runner's safety properties
//! using a fake `yt-dlp` script (runs in CI; no network). The fake mimics
//! the real tool's process shape: it spawns a long-lived child (standing in
//! for FFmpeg) so cancellation must kill the whole process tree.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use susic_import::ytdlp::{self, Options, Tools};

const FAKE: &str = r#"#!/bin/bash
here="$(cd "$(dirname "$0")" && pwd)"
if [ "$1" = "--version" ]; then echo 2026.01.01; exit 0; fi
url="${@: -1}"
out=""; prev=""
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
dest="$(dirname "$out")"
echo 'SUSIC_INFO {"id":"abcdefghijk","title":"Fake","license":"Creative Commons Attribution license (reuse allowed)"}'
case "$url" in
  *slow*)
    echo "partial" > "$dest/Fake [abcdefghijk].webm.part"
    sleep 300 &            # stands in for FFmpeg
    echo $! > "$here/grandchild.pid"
    echo "SUSIC_PROGRESS 1024 4096 100 3"
    wait ;;
  *escape*)
    echo "x" > "$dest/../outside.mp3"
    echo "SUSIC_FILE $dest/../outside.mp3" ;;
  *fail*)
    echo "ERROR: [youtube] abcdefghijk: Sign in to confirm your age." >&2; exit 1 ;;
  *)
    echo "audio" > "$dest/Fake - Fake [abcdefghijk].mp3"
    echo "SUSIC_FILE $dest/Fake - Fake [abcdefghijk].mp3" ;;
esac
"#;

fn setup(name: &str) -> (PathBuf, Tools, PathBuf) {
    let root = std::env::temp_dir().join(format!("susic-fake-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let tools = root.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    for (n, body) in [("yt-dlp", FAKE), ("ffmpeg", "#!/bin/sh\nexit 0\n")] {
        let p = tools.join(n);
        std::fs::write(&p, body).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let t = Tools::locate(Some(&tools));
    assert!(t.managed && t.ffmpeg.as_deref() == Some(tools.join("ffmpeg").as_path()));
    let dest = root.join("music/imports");
    (root, t, dest)
}

fn opts(dest: &Path) -> Options {
    Options {
        min_free_bytes: 0,
        ..Options::new(dest.to_path_buf(), "mp3", 0)
    }
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}
/// # Safety
/// Plain syscall wrapper used only for test cleanup.
unsafe fn libc_kill(pid: i32) {
    kill(pid, 9);
}

fn alive(pid: i32) -> bool {
    // A zombie still has /proc/<pid>; treat it as dead.
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|s| !s.contains(") Z "))
        .unwrap_or(false)
}

fn grandchild(root: &Path) -> i32 {
    let f = root.join("tools/grandchild.pid");
    let t0 = Instant::now();
    loop {
        if let Some(pid) = std::fs::read_to_string(&f)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            return pid;
        }
        assert!(t0.elapsed() < Duration::from_secs(10), "fake never started");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn success_records_licence_and_stays_in_folder() {
    let (root, t, dest) = setup("ok");
    let mut lic = None;
    let d = ytdlp::run(
        &t,
        "https://www.youtube.com/watch?v=abcdefghijk",
        &opts(&dest),
        Arc::new(AtomicBool::new(false)),
        &mut |e| {
            if let ytdlp::Event::Info(i) = e {
                lic = i.license;
            }
        },
    )
    .unwrap();
    assert_eq!(d.files.len(), 1);
    assert!(ytdlp::inside(&dest, &d.files[0]));
    assert!(lic.unwrap().contains("Creative Commons"));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn cancel_kills_the_whole_process_tree_and_cleans_up() {
    let (root, t, dest) = setup("cancel");
    let cancel = Arc::new(AtomicBool::new(false));
    let (t2, d2, c2) = (t.clone(), dest.clone(), cancel.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(ytdlp::run(
            &t2,
            "https://youtu.be/slowslowslo",
            &opts(&d2),
            c2,
            &mut |_| {},
        ));
    });
    let gc = grandchild(&root);
    assert!(alive(gc));
    let t0 = Instant::now();
    cancel.store(true, Ordering::SeqCst);
    // A surviving FFmpeg keeps yt-dlp's pipe open, so a broken cancel hangs:
    // fail with a deadline instead of hanging the test run.
    let r = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| {
            unsafe { libc_kill(gc) };
            panic!("cancel did not return: the FFmpeg stand-in kept running");
        });
    assert_eq!(r.unwrap_err(), "cancelled");
    assert!(
        t0.elapsed() < Duration::from_secs(3),
        "cancel took {:?}",
        t0.elapsed()
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(!alive(gc), "FFmpeg stand-in {gc} survived the cancel");
    let left: Vec<_> = std::fs::read_dir(&dest).unwrap().flatten().collect();
    assert!(left.is_empty(), "partial files left: {left:?}");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn time_limit_stops_runaway_jobs() {
    let (root, t, dest) = setup("timeout");
    let o = Options {
        max_duration: Some(Duration::from_secs(1)),
        ..opts(&dest)
    };
    let e = ytdlp::run(
        &t,
        "https://youtu.be/slowslowslo",
        &o,
        Arc::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(e.contains("took too long"), "{e}");
    let gc = grandchild(&root);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!alive(gc));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn files_outside_the_import_folder_are_refused_and_removed() {
    let (root, t, dest) = setup("escape");
    let e = ytdlp::run(
        &t,
        "https://youtu.be/escapeescap",
        &opts(&dest),
        Arc::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(e.contains("outside the import folder"), "{e}");
    assert!(!dest.join("../outside.mp3").exists());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn low_disk_space_refuses_before_downloading() {
    let (root, t, dest) = setup("disk");
    let o = Options {
        min_free_bytes: u64::MAX,
        ..opts(&dest)
    };
    let e = ytdlp::run(
        &t,
        "https://youtu.be/abcdefghijk",
        &o,
        Arc::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(e.contains("Not enough free space"), "{e}");
    assert!(!root.join("tools/grandchild.pid").exists());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn restriction_errors_are_explained_not_bypassed() {
    let (root, t, dest) = setup("fail");
    let e = ytdlp::run(
        &t,
        "https://youtu.be/failfailfai",
        &opts(&dest),
        Arc::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(e.contains("age-restricted"), "{e}");
    std::fs::remove_dir_all(root).ok();
}
