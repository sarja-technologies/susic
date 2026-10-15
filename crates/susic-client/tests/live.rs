//! Client ↔ real server over a real socket.
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use susic_client::{HttpRangeReader, Server};
use susic_server::{AppState, Config};

fn write_wav(path: &Path, secs: f32) {
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
        d.extend(((((i as f32) * 0.05).sin() * 9000.0) as i16 ^ (i % 251) as i16).to_le_bytes());
    }
    std::fs::write(path, d).unwrap();
}

struct Srv {
    state: Arc<AppState>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    th: Option<std::thread::JoinHandle<()>>,
}
fn start(state: Arc<AppState>, bind: &str) -> (Srv, std::net::SocketAddr) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let st = state.clone();
    let bind = bind.to_string();
    let th = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        rt.block_on(async move {
            let l = loop {
                match tokio::net::TcpListener::bind(&bind).await {
                    Ok(l) => break l,
                    Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            };
            tx.send(l.local_addr().unwrap()).unwrap();
            susic_server::serve(st, l, async { let _ = stop_rx.await; }).await.unwrap();
        });
    });
    let addr = rx.recv().unwrap();
    (Srv { state, stop: Some(stop), th: Some(th) }, addr)
}
impl Srv {
    fn stop(&mut self) {
        if let Some(s) = self.stop.take() { let _ = s.send(()); }
        if let Some(t) = self.th.take() { let _ = t.join(); }
    }
}

fn setup(name: &str) -> (Srv, std::net::SocketAddr, Server, i64, Vec<u8>, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("susic-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let music = root.join("music");
    std::fs::create_dir_all(&music).unwrap();
    write_wav(&music.join("big.wav"), 100.0); // ~1.6 MB: several 512 KiB chunks
    let cfg = Config { bind: "127.0.0.1:0".into(), music_dir: music.clone(), data_dir: root.join("data"), scan_interval_secs: 0, server_name: "T".into() };
    let state = susic_server::open_state(cfg).unwrap();
    state.run_scan().unwrap().unwrap();
    let code = susic_server::issue_pair_code(&state, Duration::from_secs(60)).unwrap();
    let (srv, addr) = start(state, "127.0.0.1:0");
    let base = format!("http://{addr}");
    let paired = susic_client::pair(&base, &code, "unit test").unwrap();
    let server = Server::new(&base, &paired.token);
    let delta = server.library(0).unwrap();
    assert_eq!(delta.tracks.len(), 1);
    let id = delta.tracks[0].id;
    let bytes = std::fs::read(music.join("big.wav")).unwrap();
    (srv, addr, server, id, bytes, root)
}

#[test]
fn reader_seeks_and_matches_file_and_download_resumes() {
    let (mut srv, _addr, server, id, bytes, root) = setup("reader");
    assert_eq!(susic_client::health(&server.base).unwrap().status, "ok");
    let mut r = HttpRangeReader::open(&server.stream_url(id)).unwrap();
    assert_eq!(r.len() as usize, bytes.len());
    let mut all = Vec::new();
    r.read_to_end(&mut all).unwrap();
    assert_eq!(all, bytes);
    // Random seek lands on the right bytes, crossing a chunk boundary.
    let at = 512 * 1024 - 10;
    r.seek(SeekFrom::Start(at)).unwrap();
    let mut buf = vec![0u8; 100];
    r.read_exact(&mut buf).unwrap();
    assert_eq!(buf, bytes[at as usize..at as usize + 100]);

    // Resumable download: cancel midway, then finish from the .part file.
    let dest = root.join("dl/big.wav");
    let err = server.download(id, &dest, &mut |done, _| done < 100_000);
    assert!(matches!(err, Err(susic_client::Error::Cancelled)));
    assert!(!dest.exists());
    let part_len = std::fs::metadata(root.join("dl/big.wav.part")).unwrap().len();
    assert!(part_len > 0 && part_len < bytes.len() as u64);
    server.download(id, &dest, &mut |_, _| true).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    srv.stop();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reader_survives_server_restart_mid_stream() {
    let (mut srv, addr, server, id, bytes, root) = setup("restart");
    let mut r = HttpRangeReader::open(&server.stream_url(id)).unwrap();
    let mut first = vec![0u8; 1000];
    r.read_exact(&mut first).unwrap();
    // Jump past the cached/prefetched chunks, then take the server down.
    r.seek(SeekFrom::Start(bytes.len() as u64 - 5000)).unwrap();
    let state = srv.state.clone();
    srv.stop();
    let bind = addr.to_string();
    let restarter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        start(state, &bind)
    });
    let mut tail = Vec::new();
    r.read_to_end(&mut tail).expect("reader should reconnect");
    assert_eq!(tail, bytes[bytes.len() - 5000..]);
    let (mut srv2, _) = restarter.join().unwrap();
    srv2.stop();
    let _ = std::fs::remove_dir_all(root);
}
