//! Susic server: indexes a music directory into SQLite and streams it over a
//! small token-authenticated HTTP API. See `docs/SERVER.md`.

pub mod api;
pub mod config;
pub mod db;
pub mod scan;

use rand::RngCore;
use std::sync::Arc;
use std::time::Duration;

pub use api::{router, AppState};
pub use config::Config;

pub fn log(level: &str, msg: &str) {
    // journald timestamps stderr lines itself.
    eprintln!("[{level}] {msg}");
}

pub fn random_token() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Human-friendly one-time pairing code, e.g. `K7QH-2MXP` (no 0/O/1/I).
pub fn new_pair_code() -> String {
    const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
    let mut rng = rand::thread_rng();
    let mut s = String::new();
    for i in 0..8 {
        if i == 4 {
            s.push('-');
        }
        s.push(ALPHABET[(rng.next_u32() as usize) % ALPHABET.len()] as char);
    }
    s
}

pub fn open_state(cfg: Config) -> Result<Arc<AppState>, String> {
    std::fs::create_dir_all(&cfg.data_dir)
        .map_err(|e| format!("cannot create data dir {}: {e}", cfg.data_dir.display()))?;
    let db = db::Db::open(&cfg.db_path()).map_err(|e| e.to_string())?;
    Ok(AppState::new(db, cfg))
}

/// Issue a pairing code valid for `ttl`.
pub fn issue_pair_code(state: &AppState, ttl: Duration) -> Result<String, String> {
    let code = new_pair_code();
    let hash = db::hash_secret(&api::normalise_code(&code));
    state
        .db
        .add_pair_code(&hash, db::now() + ttl.as_secs() as i64)
        .map_err(|e| e.to_string())?;
    Ok(code)
}

/// Serve until `shutdown` resolves. Also runs the periodic rescan loop.
pub async fn serve(
    state: Arc<AppState>,
    listener: tokio::net::TcpListener,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let st = state.clone();
    let interval = state.cfg.scan_interval_secs;
    let scanner = tokio::spawn(async move {
        loop {
            let s = st.clone();
            let _ = tokio::task::spawn_blocking(move || s.run_scan()).await;
            if interval == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
    let res = axum::serve(listener, router(state.clone()))
        .with_graceful_shutdown(shutdown)
        .await;
    state
        .cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);
    scanner.abort();
    res
}
