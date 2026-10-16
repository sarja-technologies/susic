use std::path::PathBuf;
use std::time::Duration;
use susic_server::{db, Config};

const USAGE: &str = "susic-server — self-hosted Susic music server

USAGE:
    susic-server [--config FILE] <COMMAND>

COMMANDS:
    serve            Run the server (default)
    scan             Index the music directory once and exit
    pair             Print a one-time pairing code (valid 10 minutes)
    devices          List paired devices
    revoke <ID>      Revoke a paired device
    purge-missing    Permanently forget tracks whose files are gone
    check            Validate configuration and paths
    ytdlp-install    Install Susic's managed, checksum-verified yt-dlp
    ytdlp-update     Update yt-dlp (managed copy, or `yt-dlp -U`)
    version

CONFIG: --config FILE (TOML), or SUSIC_BIND, SUSIC_MUSIC_DIR, SUSIC_DATA_DIR,
SUSIC_SCAN_INTERVAL, SUSIC_YTDLP_ENABLED, SUSIC_IMPORTS_DIR environment variables. Defaults to /etc/susic/server.toml if present.";

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut config_path: Option<PathBuf> = None;
    if let Some(i) = args.iter().position(|a| a == "--config" || a == "-c") {
        if i + 1 >= args.len() {
            eprintln!("--config needs a value");
            std::process::exit(2);
        }
        config_path = Some(args[i + 1].clone().into());
        args.drain(i..i + 2);
    }
    let cmd = args
        .first()
        .map(String::as_str)
        .unwrap_or("serve")
        .to_string();
    if matches!(cmd.as_str(), "-h" | "--help" | "help") {
        println!("{USAGE}");
        return;
    }
    if cmd == "version" || cmd == "--version" {
        println!("susic-server {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if config_path.is_none() {
        let default = PathBuf::from("/etc/susic/server.toml");
        if default.is_file() {
            config_path = Some(default);
        }
    }
    let cfg = match Config::load(config_path.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run(cmd, args, cfg) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cmd: String, args: Vec<String>, cfg: Config) -> Result<(), String> {
    match cmd.as_str() {
        "check" => {
            println!("bind:        {}", cfg.bind);
            println!(
                "music_dir:   {} ({})",
                cfg.music_dir.display(),
                if cfg.music_dir.is_dir() {
                    "ok"
                } else {
                    "MISSING"
                }
            );
            println!("data_dir:    {}", cfg.data_dir.display());
            println!("scan every:  {}s", cfg.scan_interval_secs);
            let t = susic_import::ytdlp::tool_status(Some(&cfg.tools_dir()));
            println!(
                "youtube:     {}",
                if cfg.ytdlp_enabled {
                    "enabled"
                } else {
                    "disabled (ytdlp_enabled = false)"
                }
            );
            if cfg.ytdlp_enabled {
                println!(
                    "  imports:   {} ({})",
                    cfg.imports_path().display(),
                    match susic_server::imports::check_writable(&cfg.imports_path()) {
                        // `check` runs outside systemd's sandbox; the service reports
                        // its own view in its startup log and GET /api/v1/youtube.
                        Ok(()) => "writable by this user; under systemd also see ReadWritePaths"
                            .to_string(),
                        Err(e) => format!("NOT WRITABLE: {e}"),
                    }
                );
                println!(
                    "  yt-dlp:    {}",
                    match (&t.ytdlp, &t.version) {
                        (Some(p), v) => format!(
                            "{p} ({}){}",
                            v.as_deref().unwrap_or("?"),
                            if t.managed { " [managed]" } else { "" }
                        ),
                        (None, _) => "MISSING — run `susic-server ytdlp-install`".into(),
                    }
                );
                println!(
                    "  ffmpeg:    {}",
                    t.ffmpeg.as_deref().unwrap_or("MISSING — install ffmpeg")
                );
                println!(
                    "  js runtime: {}",
                    t.js_runtime
                        .as_deref()
                        .unwrap_or("none (recommended: deno or node, see yt-dlp EJS wiki)")
                );
            }
            if !cfg.music_dir.is_dir() {
                return Err("music_dir does not exist or is not a directory".into());
            }
            std::fs::create_dir_all(&cfg.data_dir)
                .map_err(|e| format!("data_dir not writable: {e}"))?;
            println!("ok");
            Ok(())
        }
        "scan" => {
            let st = susic_server::open_state(cfg)?;
            match st.run_scan() {
                Some(Ok(r)) => {
                    println!("{}", serde_json::to_string_pretty(&r).unwrap());
                    Ok(())
                }
                Some(Err(e)) => Err(e),
                None => Err("scan already running".into()),
            }
        }
        "pair" => {
            let st = susic_server::open_state(cfg)?;
            let code = susic_server::issue_pair_code(&st, Duration::from_secs(600))?;
            println!("Pairing code: {code}   (valid for 10 minutes, single use)");
            Ok(())
        }
        "devices" => {
            let st = susic_server::open_state(cfg)?;
            for d in st.db.devices().map_err(|e| e.to_string())? {
                println!(
                    "#{:<3} {:<28} paired {} last seen {}{}",
                    d.id,
                    d.name,
                    d.created_at,
                    d.last_seen,
                    if d.revoked { "  [revoked]" } else { "" }
                );
            }
            Ok(())
        }
        "revoke" => {
            let id: i64 = args
                .get(1)
                .and_then(|s| s.parse().ok())
                .ok_or("usage: revoke <ID>")?;
            let st = susic_server::open_state(cfg)?;
            if st.db.revoke_device(id).map_err(|e| e.to_string())? {
                println!("device #{id} revoked");
                Ok(())
            } else {
                Err(format!("no device #{id}"))
            }
        }
        "purge-missing" => {
            let st = susic_server::open_state(cfg)?;
            println!(
                "purged {} tracks",
                st.db.purge_missing().map_err(|e| e.to_string())?
            );
            Ok(())
        }
        "ytdlp-install" => {
            let v = susic_import::ytdlp::install_managed(&cfg.tools_dir())?;
            println!("installed yt-dlp {v} to {}", cfg.tools_dir().display());
            Ok(())
        }
        "ytdlp-update" => {
            println!("{}", susic_import::ytdlp::update(&cfg.tools_dir())?);
            Ok(())
        }
        "serve" => serve(cfg),
        other => Err(format!("unknown command '{other}'\n\n{USAGE}")),
    }
}

fn serve(cfg: Config) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(8)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async move {
        let st = susic_server::open_state(cfg.clone())?;
        let listener = tokio::net::TcpListener::bind(&cfg.bind)
            .await
            .map_err(|e| format!("cannot bind {}: {e}", cfg.bind))?;
        let live = st.db.live_count().map_err(|e| e.to_string())?;
        susic_server::log(
            "info",
            &format!(
                "susic-server {} listening on {} (music: {}, {} tracks indexed)",
                env!("CARGO_PKG_VERSION"),
                cfg.bind,
                cfg.music_dir.display(),
                live
            ),
        );
        if cfg.ytdlp_enabled && cfg.music_dir.is_dir() {
            if let Err(e) = susic_server::imports::check_writable(&cfg.imports_path()) {
                susic_server::log("warn", &format!("YouTube import will fail: {e}"));
            }
        }
        if !cfg.music_dir.is_dir() {
            susic_server::log(
                "warn",
                &format!(
                    "music_dir {} is not available; serving the existing index until it appears",
                    cfg.music_dir.display()
                ),
            );
        }
        if st
            .db
            .devices()
            .map(|d| d.iter().all(|x| x.revoked))
            .unwrap_or(true)
        {
            susic_server::log(
                "info",
                "no paired devices yet: run `susic-server pair` to create a pairing code",
            );
        }
        let _ = db::now();
        susic_server::serve(st, listener, shutdown_signal())
            .await
            .map_err(|e| e.to_string())
    })
}

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = term.recv() => {},
    }
    susic_server::log("info", "shutting down");
}
