//! Optional yt-dlp integration (desktop only). yt-dlp and FFmpeg are external
//! executables found on PATH; Susic never bundles or auto-downloads them.
//! Susic does not bypass DRM, authentication or access restrictions.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(bin)).find(|p| p.is_file())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolStatus {
    pub ytdlp: Option<String>,
    pub ffmpeg: Option<String>,
}

pub fn tool_status() -> ToolStatus {
    ToolStatus {
        ytdlp: which("yt-dlp").map(|p| p.to_string_lossy().into_owned()),
        ffmpeg: which("ffmpeg").map(|p| p.to_string_lossy().into_owned()),
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub dest_dir: PathBuf,
    /// mp3 | m4a | opus | flac | wav | best
    pub format: String,
    /// kbps for lossy formats, 0 = best
    pub quality: u32,
}

#[derive(Debug, Clone)]
pub enum Event {
    Progress(f64, String),
    File(PathBuf),
}

pub fn validate_url(url: &str) -> Result<(), String> {
    let u = url.trim();
    if !(u.starts_with("https://") || u.starts_with("http://")) || u.contains(char::is_whitespace) {
        return Err("Enter a valid http(s) URL".into());
    }
    Ok(())
}

pub fn parse_progress(line: &str) -> Option<f64> {
    // "[download]  42.3% of ..."
    let rest = line.strip_prefix("[download]")?.trim_start();
    let pct = rest.split('%').next()?.trim();
    pct.parse::<f64>().ok().map(|p| (p / 100.0).clamp(0.0, 1.0))
}

pub fn build_args(url: &str, o: &Options) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--no-playlist".into(),
        "--newline".into(),
        "--no-colors".into(),
        "-x".into(),
        "--embed-metadata".into(),
        "--embed-thumbnail".into(),
        "--convert-thumbnails".into(),
        "jpg".into(),
        "--print".into(),
        "after_move:SUSIC_FILE:%(filepath)s".into(),
        "-o".into(),
        o.dest_dir.join("%(artist,uploader)s - %(title)s.%(ext)s").to_string_lossy().into_owned(),
    ];
    if o.format != "best" {
        a.push("--audio-format".into());
        a.push(o.format.clone());
    }
    if o.quality > 0 && matches!(o.format.as_str(), "mp3" | "m4a" | "opus") {
        a.push("--audio-quality".into());
        a.push(format!("{}K", o.quality));
    }
    a.push(url.to_string());
    a
}

/// Runs yt-dlp to completion, reporting progress. `cancel` kills the process.
pub fn run(
    url: &str,
    o: &Options,
    cancel: Arc<AtomicBool>,
    on_event: &mut dyn FnMut(Event),
) -> Result<Vec<PathBuf>, String> {
    validate_url(url)?;
    let bin = which("yt-dlp").ok_or("yt-dlp was not found on PATH. Install it to enable URL import.")?;
    if which("ffmpeg").is_none() {
        return Err("FFmpeg was not found on PATH. It is required to extract audio.".into());
    }
    std::fs::create_dir_all(&o.dest_dir).map_err(|e| e.to_string())?;
    let mut child: Child = Command::new(bin)
        .args(build_args(url, o))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut last = String::new();
        for l in BufReader::new(stderr).lines().map_while(Result::ok) {
            if l.contains("ERROR") {
                last = l;
            }
        }
        last
    });
    let stdout = child.stdout.take().unwrap();
    let mut files = Vec::new();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
        if let Some(p) = line.strip_prefix("SUSIC_FILE:") {
            let p = PathBuf::from(p.trim());
            on_event(Event::File(p.clone()));
            files.push(p);
        } else if let Some(p) = parse_progress(&line) {
            on_event(Event::Progress(p, line.trim().to_string()));
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_thread.join().unwrap_or_default();
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if !status.success() {
        return Err(if err.is_empty() { format!("yt-dlp exited with {status}") } else { err });
    }
    Ok(files)
}

pub fn dest_or_default(p: Option<&Path>, fallback: &Path) -> PathBuf {
    p.map(Path::to_path_buf).unwrap_or_else(|| fallback.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_parse() {
        assert_eq!(parse_progress("[download]  42.5% of 3.00MiB at 1MiB/s"), Some(0.425));
        assert_eq!(parse_progress("[info] x"), None);
    }

    #[test]
    fn url_validation() {
        assert!(validate_url("https://example.com/a").is_ok());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("--exec rm").is_err());
    }
}
