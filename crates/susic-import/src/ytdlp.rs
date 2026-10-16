//! Optional yt-dlp integration: YouTube search through yt-dlp's `ytsearch`
//! (no Google account or API key), audio import with yt-dlp + FFmpeg, and
//! management of the yt-dlp executable.
//!
//! Policy: Susic acts only on explicit user requests, never passes cookies or
//! account credentials to yt-dlp, and never tries to get around DRM, sign-in,
//! age, membership, payment or region restrictions — those failures are
//! reported as such. Users are responsible for only importing content they
//! have the right to download.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const AUDIO_EXTS: &[&str] = &["mp3", "m4a", "opus", "ogg", "flac", "wav", "webm", "aac"];

// ---- locating tools -----------------------------------------------------------

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Directories GUI apps often miss because they are only added to PATH by a
/// login shell (pipx, Homebrew, deno installers).
fn extra_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        v.push(home.join(".local/bin"));
        v.push(home.join(".deno/bin"));
        v.push(home.join(".bun/bin"));
    }
    for d in [
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/usr/bin",
        "/snap/bin",
    ] {
        v.push(PathBuf::from(d));
    }
    v
}

/// Find `bin` on PATH, then in common per-user install locations.
pub fn which(bin: &str) -> Option<PathBuf> {
    let name = exe(bin);
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(extra_dirs())
        .map(|d| d.join(&name))
        .find(|p| p.is_file())
}

/// The external programs Susic drives.
#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub ytdlp: Option<PathBuf>,
    /// yt-dlp is Susic's own checksum-verified copy (updatable in-app).
    pub managed: bool,
    pub ffmpeg: Option<PathBuf>,
    /// JavaScript runtime yt-dlp uses for YouTube (deno, node or bun).
    pub js_runtime: Option<(String, PathBuf)>,
    /// yt-dlp cache directory (default: yt-dlp's own, under the user's home).
    pub cache_dir: Option<PathBuf>,
}

pub fn managed_path(managed_dir: &Path) -> PathBuf {
    managed_dir.join(exe("yt-dlp"))
}

impl Tools {
    /// Prefer the managed copy in `managed_dir` (installed from Susic), then PATH.
    pub fn locate(managed_dir: Option<&Path>) -> Self {
        let managed = managed_dir.map(managed_path).filter(|p| p.is_file());
        let is_managed = managed.is_some();
        let js_runtime = ["deno", "node", "bun"]
            .iter()
            .find_map(|n| which(n).map(|p| (n.to_string(), p)));
        Tools {
            ytdlp: managed.or_else(|| which("yt-dlp")),
            managed: is_managed,
            // An ffmpeg placed next to the managed yt-dlp wins (useful on
            // servers without a packaged FFmpeg).
            ffmpeg: managed_dir
                .map(|d| d.join(exe("ffmpeg")))
                .filter(|p| p.is_file())
                .or_else(|| which("ffmpeg")),
            js_runtime,
            cache_dir: None,
        }
    }

    fn ytdlp(&self) -> Result<&Path, String> {
        self.ytdlp.as_deref().ok_or_else(|| {
            "yt-dlp is not installed. Install it from Settings → YouTube, or with your package manager."
                .to_string()
        })
    }

    /// Flags shared by every invocation. `--ignore-config` keeps a user's own
    /// yt-dlp config (output templates, cookies) from changing Susic's parsing.
    fn base_args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["--ignore-config".into(), "--no-colors".into()];
        if let Some(d) = &self.cache_dir {
            a.push("--cache-dir".into());
            a.push(d.to_string_lossy().into_owned());
        }
        if let (Some((name, path)), Some(v)) = (
            &self.js_runtime,
            self.ytdlp.as_deref().and_then(cached_version),
        ) {
            // --js-runtimes appeared in 2025.11; older builds reject it.
            if v.as_str() >= "2025.11" {
                a.push("--js-runtimes".into());
                a.push(format!("{name}:{}", path.display()));
            }
        }
        if let Some(dir) = self.ffmpeg.as_deref().and_then(Path::parent) {
            a.push("--ffmpeg-location".into());
            a.push(dir.to_string_lossy().into_owned());
        }
        a
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub ytdlp: Option<String>,
    pub version: Option<String>,
    pub managed: bool,
    pub ffmpeg: Option<String>,
    pub js_runtime: Option<String>,
    /// A managed copy can be installed on this platform.
    pub can_install: bool,
}

pub fn tool_status(managed_dir: Option<&Path>) -> ToolStatus {
    let t = Tools::locate(managed_dir);
    ToolStatus {
        version: t.ytdlp.as_deref().and_then(cached_version),
        ytdlp: t.ytdlp.map(|p| p.to_string_lossy().into_owned()),
        managed: t.managed,
        ffmpeg: t.ffmpeg.map(|p| p.to_string_lossy().into_owned()),
        js_runtime: t.js_runtime.map(|(n, _)| n),
        can_install: release_asset().is_some(),
    }
}

/// `yt-dlp --version`, cached per binary path + mtime (the PyInstaller
/// build takes ~1 s to start).
pub fn cached_version(bin: &Path) -> Option<String> {
    type Cache = Mutex<HashMap<PathBuf, (std::time::SystemTime, String)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let mtime = std::fs::metadata(bin).and_then(|m| m.modified()).ok()?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some((t, v)) = cache.lock().unwrap().get(bin) {
        if *t == mtime {
            return Some(v.clone());
        }
    }
    let out = run_capture(Command::new(bin).arg("--version"), Duration::from_secs(30)).ok()?;
    if !out.status.success() {
        return None;
    }
    let v = out.stdout.trim().to_string();
    cache
        .lock()
        .unwrap()
        .insert(bin.to_path_buf(), (mtime, v.clone()));
    Some(v)
}

struct Captured {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// Run to completion with a hard timeout (the process is killed on expiry).
fn run_capture(cmd: &mut Command, timeout: Duration) -> Result<Captured, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {:?}: {e}", cmd.get_program()))?;
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let t1 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let t2 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "timed out after {}s — check your connection and try again",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    Ok(Captured {
        status,
        stdout: t1.join().unwrap_or_default(),
        stderr: t2.join().unwrap_or_default(),
    })
}

// ---- install / update ---------------------------------------------------------

/// The official standalone release asset for this platform.
pub fn release_asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("yt-dlp_linux"),
        ("linux", "aarch64") => Some("yt-dlp_linux_aarch64"),
        ("macos", _) => Some("yt-dlp_macos"),
        ("windows", "x86_64") => Some("yt-dlp.exe"),
        ("windows", "aarch64") => Some("yt-dlp_arm64.exe"),
        _ => None,
    }
}

/// Find `asset`'s digest in a `SHA2-256SUMS` file.
pub fn expected_sha256(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, name) = (it.next()?, it.next()?);
        (name.trim_start_matches('*') == asset && h.len() == 64).then(|| h.to_lowercase())
    })
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .user_agent(concat!("Susic/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Download the latest official yt-dlp release into `managed_dir`, verifying
/// it against the release's SHA2-256SUMS. Returns the installed version.
pub fn install_managed(managed_dir: &Path) -> Result<String, String> {
    use sha2::Digest;
    let asset = release_asset().ok_or(
        "No official yt-dlp build for this platform. Install yt-dlp with your package manager.",
    )?;
    let agent = http_agent();
    let net = |e: ureq::Error| format!("Download failed: {e}");
    let sums = agent
        .get(&format!("{RELEASES}/SHA2-256SUMS"))
        .call()
        .map_err(net)?
        .into_string()
        .map_err(|e| e.to_string())?;
    let want = expected_sha256(&sums, asset)
        .ok_or("Release checksum list has no entry for this platform")?;
    let resp = agent
        .get(&format!("{RELEASES}/{asset}"))
        .call()
        .map_err(net)?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(300 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Download failed: {e}"))?;
    let got = format!("{:x}", sha2::Sha256::digest(&bytes));
    if got != want {
        return Err(
            "Downloaded yt-dlp failed checksum verification; nothing was installed.".into(),
        );
    }
    std::fs::create_dir_all(managed_dir).map_err(|e| e.to_string())?;
    let dest = managed_path(managed_dir);
    let tmp = dest.with_extension("download");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    cached_version(&dest).ok_or_else(|| "Installed yt-dlp does not run on this system".into())
}

/// Latest published release tag (e.g. `2026.08.19`).
pub fn latest_version() -> Result<String, String> {
    let v: serde_json::Value = http_agent()
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Could not check for updates: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    v["tag_name"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "Could not check for updates".into())
}

/// Update yt-dlp. The managed copy is replaced with the verified latest
/// release; a system copy is updated with `yt-dlp -U` where its installer
/// allows it (pip/apt installs say how to update instead).
pub fn update(managed_dir: &Path) -> Result<String, String> {
    let t = Tools::locate(Some(managed_dir));
    let bin = t.ytdlp()?.to_path_buf();
    if t.managed {
        let current = cached_version(&bin).unwrap_or_default();
        if let Ok(latest) = latest_version() {
            if latest == current {
                return Ok(format!("yt-dlp {current} is already the latest version"));
            }
        }
        let v = install_managed(managed_dir)?;
        return Ok(format!("Updated yt-dlp to {v}"));
    }
    let out = run_capture(
        Command::new(&bin).args(["--ignore-config", "-U"]),
        Duration::from_secs(300),
    )?;
    let text = format!("{}\n{}", out.stdout, out.stderr);
    let last = text
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("")
        .trim_start_matches("ERROR: ")
        .to_string();
    if out.status.success() {
        Ok(if last.is_empty() {
            "yt-dlp is up to date".into()
        } else {
            last
        })
    } else {
        Err(format!(
            "{last}{}",
            if release_asset().is_some() {
                " — or install Susic's own copy, which Susic can update for you."
            } else {
                ""
            }
        ))
    }
}

// ---- search -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub url: String,
    pub title: String,
    /// Artist when the source provides one, else the channel name.
    pub artist: String,
    pub channel: String,
    pub duration_ms: Option<i64>,
    pub thumb: String,
    pub live: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    pub limit: usize,
    /// Restrict to videos their uploaders license under Creative Commons
    /// (YouTube's own search filter).
    pub creative_commons: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 12,
            creative_commons: false,
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// What to hand yt-dlp for a search.
pub fn search_target(query: &str, o: &SearchOptions) -> String {
    let q = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if o.creative_commons {
        // sp=EgIwAQ== is YouTube's "Creative Commons" search filter.
        format!(
            "https://www.youtube.com/results?search_query={}&sp=EgIwAQ%3D%3D",
            urlencode(&q)
        )
    } else {
        format!("ytsearch{}:{q}", o.limit.clamp(1, 30))
    }
}

fn strip_topic(s: &str) -> String {
    s.trim().trim_end_matches(" - Topic").trim().to_string()
}

/// Parse `yt-dlp --flat-playlist -J` output.
pub fn parse_search(json: &str) -> Result<Vec<SearchResult>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "yt-dlp returned unreadable search results")?;
    let entries = v["entries"].as_array().cloned().unwrap_or_default();
    let s = |e: &serde_json::Value, k: &str| e[k].as_str().unwrap_or("").trim().to_string();
    Ok(entries
        .iter()
        .filter_map(|e| {
            let id = s(e, "id");
            let ie = s(e, "ie_key");
            // Channels and playlists also appear in result pages.
            if id.is_empty() || (!ie.is_empty() && ie != "Youtube") {
                return None;
            }
            let live_status = s(e, "live_status");
            if live_status == "is_upcoming" {
                return None;
            }
            let channel = [s(e, "channel"), s(e, "uploader")]
                .into_iter()
                .find(|x| !x.is_empty())
                .unwrap_or_default();
            let artist = [s(e, "artist"), s(e, "creator")]
                .into_iter()
                .find(|x| !x.is_empty())
                .unwrap_or_else(|| strip_topic(&channel));
            let url = Some(s(e, "url"))
                .filter(|u| u.starts_with("http"))
                .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
            let thumb = if ie == "Youtube" || url.contains("youtube.com/") {
                format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")
            } else {
                e["thumbnails"]
                    .as_array()
                    .and_then(|t| t.last())
                    .map(|t| s(t, "url"))
                    .unwrap_or_default()
            };
            Some(SearchResult {
                title: s(e, "title"),
                duration_ms: e["duration"].as_f64().map(|d| (d * 1000.0) as i64),
                live: live_status == "is_live" || e["is_live"].as_bool() == Some(true),
                id,
                url,
                artist,
                channel,
                thumb,
            })
        })
        .collect())
}

/// Search YouTube with yt-dlp. Needs no API key or Google account.
pub fn search(tools: &Tools, query: &str, o: &SearchOptions) -> Result<Vec<SearchResult>, String> {
    let bin = tools.ytdlp()?;
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    let mut args = tools.base_args();
    args.extend(
        [
            "--flat-playlist",
            "-J",
            "--no-warnings",
            "--socket-timeout",
            "15",
            "--extractor-retries",
            "2",
            "--playlist-end",
        ]
        .map(String::from),
    );
    args.push(o.limit.clamp(1, 30).to_string());
    args.push("--".into());
    args.push(search_target(query, o));
    let out = run_capture(Command::new(bin).args(&args), Duration::from_secs(45))
        .map_err(|e| format!("YouTube search {e}"))?;
    if !out.status.success() {
        return Err(friendly_error(&out.stderr));
    }
    let mut r = parse_search(&out.stdout)?;
    r.truncate(o.limit.clamp(1, 30));
    Ok(r)
}

// ---- import -------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Options {
    pub dest_dir: PathBuf,
    /// mp3 | m4a | opus | flac | wav | best
    pub format: String,
    /// kbps for lossy formats, 0 = best
    pub quality: u32,
    /// Refuse to start unless the destination has at least this much free space.
    pub min_free_bytes: u64,
    /// Kill the job if it runs longer than this.
    pub max_duration: Option<Duration>,
}

impl Default for Options {
    fn default() -> Self {
        Self::new(PathBuf::new(), "m4a", 0)
    }
}

impl Options {
    pub fn new(dest_dir: PathBuf, format: &str, quality: u32) -> Self {
        Self {
            dest_dir,
            format: format.into(),
            quality,
            min_free_bytes: DEFAULT_MIN_FREE,
            max_duration: Some(DEFAULT_MAX_DURATION),
        }
    }
}

/// Per-file cap passed to yt-dlp (`--max-filesize`).
pub const MAX_FILE_BYTES: u64 = 1 << 30;
/// Default free-space reserve: the 1 GiB file cap plus 1 GiB headroom.
pub const DEFAULT_MIN_FREE: u64 = 2 << 30;
pub const DEFAULT_MAX_DURATION: Duration = Duration::from_secs(2 * 3600);

/// Bytes available to unprivileged users on the filesystem holding `p`.
pub fn free_space(p: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
        // SAFETY: statvfs writes into a zeroed struct we own; c is NUL-terminated.
        unsafe {
            let mut s: libc::statvfs = std::mem::zeroed();
            (libc::statvfs(c.as_ptr(), &mut s) == 0)
                .then(|| (s.f_bavail as u64).saturating_mul(s.f_frsize as u64))
        }
    }
    #[cfg(not(unix))]
    {
        let _ = p;
        None
    }
}

/// Kill yt-dlp *and* its children (FFmpeg) so nothing keeps writing after a
/// cancel. yt-dlp is started as the leader of its own process group.
fn kill_tree(pid: u32) {
    #[cfg(unix)]
    // SAFETY: plain syscall; a negative pid addresses the process group.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .status();
    }
}

/// True if `p` is a regular file (not a symlink) inside `dir`.
pub fn inside(dir: &Path, p: &Path) -> bool {
    let (Ok(d), Ok(f)) = (dir.canonicalize(), p.canonicalize()) else {
        return false;
    };
    let regular = std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_file())
        .unwrap_or(false);
    regular && f.starts_with(&d) && f != d
}

pub const FORMATS: &[&str] = &["mp3", "m4a", "opus", "flac", "wav", "best"];

/// Facts about the item, printed by yt-dlp before downloading.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// Licence as declared by the uploader (YouTube reports Creative Commons here).
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub webpage_url: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Info(MediaInfo),
    /// Overall fraction 0..1 and a human-readable status line.
    Progress(f64, String),
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Downloaded {
    pub files: Vec<PathBuf>,
    pub info: Option<MediaInfo>,
    /// The file already existed in the destination; nothing was downloaded.
    pub reused: bool,
}

pub fn validate_url(url: &str) -> Result<(), String> {
    let u = url.trim();
    if !(u.starts_with("https://") || u.starts_with("http://")) || u.contains(char::is_whitespace) {
        return Err("Enter a valid http(s) URL".into());
    }
    let lower = u.to_ascii_lowercase();
    if (lower.contains("youtube.com/playlist") || lower.contains("list="))
        && youtube_id(u).is_none()
    {
        return Err("Playlist import isn't supported — import songs one at a time.".into());
    }
    Ok(())
}

/// The 11-character video id of a YouTube URL, if it is one.
pub fn youtube_id(url: &str) -> Option<String> {
    let u = url.trim();
    let host_rest = u.split("://").nth(1)?;
    let (host, rest) = host_rest.split_once('/').unwrap_or((host_rest, ""));
    let host = host.trim_start_matches("www.").trim_start_matches("m.");
    let id = match host {
        "youtu.be" => rest.split(['?', '&', '#', '/']).next()?.to_string(),
        "youtube.com" | "music.youtube.com" => {
            if let Some(q) = rest.strip_prefix("watch?").or_else(|| {
                rest.split_once('?')
                    .filter(|(p, _)| *p == "watch")
                    .map(|(_, q)| q)
            }) {
                q.split('&')
                    .find_map(|kv| kv.strip_prefix("v="))?
                    .to_string()
            } else {
                let mut parts = rest.split('/');
                match parts.next()? {
                    "shorts" | "live" | "embed" => {
                        parts.next()?.split(['?', '&', '#']).next()?.to_string()
                    }
                    _ => return None,
                }
            }
        }
        _ => return None,
    };
    (id.len() == 11
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    .then_some(id)
}

/// An already-imported copy of `id` in `dir` (Susic names files `… [id].ext`).
pub fn existing_import(dir: &Path, id: &str) -> Option<PathBuf> {
    let tag = format!("[{id}].");
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            name.contains(&tag) && AUDIO_EXTS.contains(&ext.as_str())
        })
}

/// Square-crop the (usually 16:9) video thumbnail used as cover art.
const SQUARE_CROP: &str =
    "ThumbnailsConvertor+FFmpeg_o:-c:v mjpeg -vf crop=\"'if(gt(ih,iw),iw,ih)':'if(gt(iw,ih),ih,iw)'\"";

pub fn build_args(tools: &Tools, url: &str, o: &Options) -> Vec<String> {
    let mut a = tools.base_args();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    a.extend(s(&[
        "--no-playlist",
        "--playlist-items",
        "1",
        "--newline",
        "--no-quiet",
        "--progress",
        "--no-simulate",
        "--no-mtime",
        "--windows-filenames",
        // Be gentle with the source: back off and retry instead of hammering.
        "--socket-timeout",
        "30",
        "--retries",
        "5",
        "--fragment-retries",
        "5",
        "--extractor-retries",
        "3",
        "--retry-sleep",
        "http:exp=1:30",
        "--retry-sleep",
        "extractor:exp=1:30",
        "--max-filesize",
        "1024M",
        "-x",
        "--embed-metadata",
        "--embed-thumbnail",
        "--convert-thumbnails",
        "jpg",
        "--ppa",
        SQUARE_CROP,
        // Artist tag: the source's artist field, else the channel without " - Topic".
        "--parse-metadata",
        "%(artist,creator,uploader,channel)s:%(meta_artist)s",
        "--parse-metadata",
        "meta_artist:(?P<meta_artist>.+?)(?: - Topic)?$",
        "--progress-template",
        "download:SUSIC_PROGRESS %(progress.downloaded_bytes)s %(progress.total_bytes,progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s",
        "--print",
        "before_dl:SUSIC_INFO %(.{id,title,license,webpage_url,thumbnail})j",
        "--print",
        "after_move:SUSIC_FILE %(filepath)s",
    ]));
    // "Original" prefers AAC/M4A: desktop playback can't decode Opus.
    a.push("-f".into());
    a.push(
        if o.format == "best" {
            "bestaudio[ext=m4a]/bestaudio/best"
        } else {
            "bestaudio/best"
        }
        .into(),
    );
    a.push("-o".into());
    a.push(
        o.dest_dir
            .join("%(artist,uploader).80B - %(title).150B [%(id)s].%(ext)s")
            .to_string_lossy()
            .into_owned(),
    );
    if o.format != "best" {
        a.push("--audio-format".into());
        a.push(o.format.clone());
    }
    if matches!(o.format.as_str(), "mp3" | "m4a" | "opus") {
        a.push("--audio-quality".into());
        a.push(if o.quality > 0 {
            format!("{}K", o.quality)
        } else {
            "0".into()
        });
    }
    a.push("--".into());
    a.push(url.trim().to_string());
    a
}

fn mb(b: f64) -> String {
    format!("{:.1} MB", b / 1_048_576.0)
}

/// Parse a `SUSIC_PROGRESS done total speed eta` line into (fraction, message).
pub fn parse_progress(line: &str) -> Option<(f64, String)> {
    let mut it = line.strip_prefix("SUSIC_PROGRESS ")?.split_whitespace();
    let num = |s: Option<&str>| s.and_then(|v| v.parse::<f64>().ok());
    let done = num(it.next())?;
    let total = num(it.next()).filter(|t| *t > 0.0);
    let speed = num(it.next());
    let eta = num(it.next());
    let frac = total.map(|t| (done / t).clamp(0.0, 1.0)).unwrap_or(0.0);
    let mut msg = match total {
        Some(t) => format!("Downloading {} of {}", mb(done), mb(t)),
        None => format!("Downloading {}", mb(done)),
    };
    if let Some(s) = speed.filter(|s| *s > 0.0) {
        msg.push_str(&format!(" · {}/s", mb(s)));
    }
    if let Some(e) = eta {
        msg.push_str(&format!(" · {}:{:02} left", e as u64 / 60, e as u64 % 60));
    }
    Some((frac, msg))
}

/// Human-readable stage for yt-dlp's post-processing lines.
fn stage(line: &str) -> Option<&'static str> {
    if line.starts_with("[ExtractAudio]") {
        Some("Converting audio…")
    } else if line.starts_with("[Metadata]") {
        Some("Writing tags…")
    } else if line.starts_with("[ThumbnailsConvertor]") {
        Some("Preparing artwork…")
    } else if line.starts_with("[EmbedThumbnail]") {
        Some("Adding artwork…")
    } else if line.starts_with("[youtube]") || line.starts_with("[info]") {
        Some("Fetching details…")
    } else {
        None
    }
}

/// YouTube intermittently answers 403 for media URLs; one retry usually works.
const FORBIDDEN_MSG: &str = "YouTube refused the download (HTTP 403). This is often temporary — try again, and update yt-dlp if it keeps happening.";

/// Turn yt-dlp's stderr into a message that says what happened and what, if
/// anything, the user can do. Restrictions are reported, never worked around.
pub fn friendly_error(stderr: &str) -> String {
    let err_line = stderr
        .lines()
        .rev()
        .find(|l| l.contains("ERROR"))
        .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()))
        .unwrap_or("")
        .trim();
    let l = stderr.to_lowercase();
    let has = |s: &str| l.contains(s);
    let msg = if has("drm") {
        "This item is DRM-protected. Susic does not bypass DRM, so it can't be imported."
    } else if has("confirm your age")
        || has("age-restricted")
        || has("inappropriate for some users")
    {
        "This video is age-restricted and needs a signed-in account. Susic doesn't use your account, so it can't be imported."
    } else if has("not a bot") {
        "YouTube is asking this network to prove it isn't a bot (usually after many requests). Wait a while and try again, or update yt-dlp. Susic does not use account cookies to get around this."
    } else if has("http error 429")
        || has("too many requests")
        || has("rate-limit")
        || has("rate limit")
    {
        "YouTube is rate-limiting this network. Wait a few minutes before searching or importing again."
    } else if has("read-only file system") || has("[errno 30]") {
        "The import folder is read-only for Susic. On a server running under systemd, add the folder to ReadWritePaths (see docs/SERVER.md)."
    } else if has("permission denied") || has("[errno 13]") {
        "Susic isn't allowed to write to the import folder. Check its owner and permissions (and ReadWritePaths under systemd)."
    } else if has("no space left on device") || has("[errno 28]") {
        "The disk is full."
    } else if has("http error 403") || has("403: forbidden") {
        FORBIDDEN_MSG
    } else if has("private video") {
        "This video is private."
    } else if has("members-only") || has("join this channel") || has("members only") {
        "This video is for channel members only, so it can't be imported."
    } else if has("requires payment") || has("purchase") || has("rental") {
        "This item requires a purchase or rental, so it can't be imported."
    } else if has("not available in your country") || has("geo restrict") || has("geo-restrict") {
        "This video isn't available in your region."
    } else if has("premieres in") || has("live event will begin") || has("is_upcoming") {
        "This video hasn't been released yet."
    } else if (has("live stream") && has("not supported")) || has("is live") {
        "Live streams can't be imported."
    } else if has("unsupported url") {
        "yt-dlp doesn't support this URL."
    } else if has("larger than max-filesize") {
        "This file is larger than Susic's 1 GB import limit."
    } else if (has("ffmpeg") || has("ffprobe")) && (has("not found") || has("not installed")) {
        "FFmpeg is required to extract audio but wasn't found. Install FFmpeg and try again."
    } else if has("video unavailable")
        || has("this video is unavailable")
        || has("has been removed")
        || has("does not exist")
    {
        "This video is unavailable (removed, blocked or the link is wrong)."
    } else if has("unable to extract")
        || has("nsig")
        || has("signature")
        || has("requested format is not available")
        || has("please report this issue")
    {
        "yt-dlp couldn't read this page — YouTube changes often. Update yt-dlp (Settings → YouTube) and try again."
    } else if has("timed out")
        || has("name resolution")
        || has("getaddrinfo")
        || has("network is unreachable")
        || has("connection refused")
        || has("connection reset")
    {
        "Network error — check your internet connection and try again."
    } else {
        ""
    };
    if !msg.is_empty() {
        return msg.to_string();
    }
    let raw = err_line.trim_start_matches("ERROR:").trim();
    // "[youtube] abc: message" → "message" (but keep Python's "[Errno N] …")
    let raw = Some(raw)
        .filter(|r| !r.starts_with("[Errno"))
        .and_then(|r| r.strip_prefix('['))
        .and_then(|r| r.split_once("] "))
        .map(|(_, r)| r.split_once(": ").map(|(_, m)| m).unwrap_or(r))
        .unwrap_or(raw);
    if raw.is_empty() {
        "yt-dlp failed without an error message".into()
    } else {
        raw.to_string()
    }
}

/// Remove partial/intermediate files for `id` created by this run.
fn cleanup_partials(dir: &Path, id: &str, keep: &[PathBuf], before: &[PathBuf]) {
    let tag = format!("[{id}]");
    if let Ok(rd) = std::fs::read_dir(dir) {
        for p in rd.flatten().map(|e| e.path()) {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.contains(&tag) && !keep.contains(&p) && !before.contains(&p) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

/// Download `url` as audio into `o.dest_dir`, reporting progress. Setting
/// `cancel` kills yt-dlp promptly (even while it is silent) and removes its
/// partial files. A transient HTTP 403 is retried once after a short pause.
pub fn run(
    tools: &Tools,
    url: &str,
    o: &Options,
    cancel: Arc<AtomicBool>,
    on_event: &mut dyn FnMut(Event),
) -> Result<Downloaded, String> {
    match run_once(tools, url, o, cancel.clone(), on_event) {
        Err(e) if e == FORBIDDEN_MSG && !cancel.load(Ordering::Relaxed) => {
            on_event(Event::Progress(
                0.0,
                "YouTube refused the request — retrying…".into(),
            ));
            for _ in 0..30 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            run_once(tools, url, o, cancel, on_event)
        }
        r => r,
    }
}

fn run_once(
    tools: &Tools,
    url: &str,
    o: &Options,
    cancel: Arc<AtomicBool>,
    on_event: &mut dyn FnMut(Event),
) -> Result<Downloaded, String> {
    validate_url(url)?;
    if !FORMATS.contains(&o.format.as_str()) {
        return Err(format!("Unsupported audio format '{}'", o.format));
    }
    let bin = tools.ytdlp()?.to_path_buf();
    if tools.ffmpeg.is_none() {
        return Err(
            "FFmpeg was not found. It is required to extract audio — install it and try again."
                .into(),
        );
    }
    std::fs::create_dir_all(&o.dest_dir)
        .map_err(|e| format!("Cannot write to {}: {e}", o.dest_dir.display()))?;
    let known_id = youtube_id(url);
    if let Some(p) = known_id
        .as_deref()
        .and_then(|id| existing_import(&o.dest_dir, id))
    {
        on_event(Event::File(p.clone()));
        return Ok(Downloaded {
            files: vec![p],
            info: None,
            reused: true,
        });
    }
    if let Some(free) = free_space(&o.dest_dir) {
        if free < o.min_free_bytes {
            return Err(format!(
                "Not enough free space: {:.1} GB free, Susic keeps at least {:.1} GB free for imports.",
                free as f64 / 1e9,
                o.min_free_bytes as f64 / 1e9
            ));
        }
    }
    let before: Vec<PathBuf> = std::fs::read_dir(&o.dest_dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();

    let mut cmd = Command::new(&bin);
    cmd.args(build_args(tools, url, o))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child: Child = cmd
        .spawn()
        .map_err(|e| format!("Could not start yt-dlp: {e}"))?;
    let pid = child.id();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (finished, cancel, timed_out) = (finished.clone(), cancel.clone(), timed_out.clone());
        let limit = o.max_duration;
        let start = Instant::now();
        std::thread::spawn(move || {
            while !finished.load(Ordering::Relaxed) {
                let over = limit.is_some_and(|l| start.elapsed() > l);
                if over {
                    timed_out.store(true, Ordering::SeqCst);
                }
                if over || cancel.load(Ordering::Relaxed) {
                    kill_tree(pid);
                    return;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        })
    };
    let err_thread = std::thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        for l in BufReader::new(stderr).lines().map_while(Result::ok) {
            tail.push(l);
            if tail.len() > 40 {
                tail.remove(0);
            }
        }
        tail.join("\n")
    });

    let mut files = Vec::new();
    let mut info: Option<MediaInfo> = None;
    let mut downloaded = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(j) = line.strip_prefix("SUSIC_INFO ") {
            if let Ok(i) = serde_json::from_str::<MediaInfo>(j) {
                on_event(Event::Info(i.clone()));
                info = Some(i);
            }
        } else if let Some(p) = line.strip_prefix("SUSIC_FILE ") {
            let p = PathBuf::from(p.trim());
            on_event(Event::File(p.clone()));
            files.push(p);
        } else if let Some((f, msg)) = parse_progress(&line) {
            downloaded = true;
            on_event(Event::Progress(f * 0.9, msg));
        } else if let Some(s) = stage(&line) {
            let f = if downloaded { 0.92 } else { 0.0 };
            on_event(Event::Progress(f, s.to_string()));
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    finished.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    if cancel.load(Ordering::Relaxed) || timed_out.load(Ordering::Relaxed) {
        // Make sure no grandchild (FFmpeg) is still writing before cleanup.
        kill_tree(pid);
        std::thread::sleep(Duration::from_millis(200));
    }
    let err = err_thread.join().unwrap_or_default();
    let id = info.as_ref().map(|i| i.id.clone()).or(known_id);
    let cleanup = |keep: &[PathBuf]| {
        if let Some(id) = id.as_deref().filter(|i| !i.is_empty()) {
            cleanup_partials(&o.dest_dir, id, keep, &before);
        }
    };
    if cancel.load(Ordering::Relaxed) {
        cleanup(&[]);
        return Err("cancelled".into());
    }
    if timed_out.load(Ordering::Relaxed) {
        cleanup(&[]);
        return Err("The import took too long and was stopped.".into());
    }
    if !status.success() {
        cleanup(&[]);
        return Err(friendly_error(&err));
    }
    // Defence in depth: only accept regular files inside the destination.
    if let Some(f) = files.iter().find(|f| f.exists() && !inside(&o.dest_dir, f)) {
        let _ = std::fs::remove_file(f);
        cleanup(&[]);
        return Err(format!(
            "yt-dlp wrote outside the import folder ({}); refused.",
            f.display()
        ));
    }
    files.retain(|p| p.is_file());
    if files.is_empty() {
        cleanup(&[]);
        return Err(if err.trim().is_empty() {
            "yt-dlp finished but produced no audio file".into()
        } else {
            friendly_error(&err)
        });
    }
    cleanup(&files);
    Ok(Downloaded {
        files,
        info,
        reused: false,
    })
}

/// Fetch a thumbnail (for cover art when embedding failed). Small and bounded.
pub fn fetch_image(url: &str) -> Option<Vec<u8>> {
    if !url.starts_with("https://") {
        return None;
    }
    let resp = http_agent()
        .get(url)
        .timeout(Duration::from_secs(15))
        .call()
        .ok()?;
    let mut b = Vec::new();
    resp.into_reader()
        .take(10 * 1024 * 1024)
        .read_to_end(&mut b)
        .ok()?;
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_parse() {
        let (f, m) = parse_progress("SUSIC_PROGRESS 1048576 4194304 524288.0 6").unwrap();
        assert!((f - 0.25).abs() < 1e-9);
        assert_eq!(m, "Downloading 1.0 MB of 4.0 MB · 0.5 MB/s · 0:06 left");
        let (f, m) = parse_progress("SUSIC_PROGRESS 2048 NA NA NA").unwrap();
        assert_eq!(f, 0.0);
        assert_eq!(m, "Downloading 0.0 MB");
        assert!(parse_progress("[download] 42.5% of 3MiB").is_none());
    }

    #[test]
    fn url_validation() {
        assert!(validate_url("https://example.com/a").is_ok());
        assert!(validate_url("https://www.youtube.com/watch?v=aqz-KE-bpKQ&list=PL1").is_ok());
        assert!(validate_url("https://www.youtube.com/playlist?list=PL123").is_err());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("--exec rm").is_err());
    }

    #[test]
    fn youtube_ids() {
        for u in [
            "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
            "https://youtube.com/watch?feature=x&v=aqz-KE-bpKQ",
            "https://youtu.be/aqz-KE-bpKQ?t=3",
            "https://m.youtube.com/shorts/aqz-KE-bpKQ",
            "https://music.youtube.com/watch?v=aqz-KE-bpKQ&si=1",
        ] {
            assert_eq!(youtube_id(u).as_deref(), Some("aqz-KE-bpKQ"), "{u}");
        }
        assert_eq!(youtube_id("https://example.com/watch?v=aqz-KE-bpKQ"), None);
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=short"), None);
    }

    #[test]
    fn search_targets() {
        let o = SearchOptions {
            limit: 5,
            creative_commons: false,
        };
        assert_eq!(search_target("  daft   punk ", &o), "ytsearch5:daft punk");
        let o = SearchOptions {
            limit: 5,
            creative_commons: true,
        };
        assert_eq!(
            search_target("a&b c", &o),
            "https://www.youtube.com/results?search_query=a%26b+c&sp=EgIwAQ%3D%3D"
        );
    }

    #[test]
    fn parses_flat_search_output() {
        let j = r#"{"entries":[
          {"id":"abcdefghijk","ie_key":"Youtube","title":"Song","channel":"Band - Topic","duration":185.0,"url":"https://www.youtube.com/watch?v=abcdefghijk"},
          {"id":"UCxyz","ie_key":"YoutubeTab","title":"A channel","url":"https://www.youtube.com/channel/UCxyz"},
          {"id":"zzzzzzzzzzz","ie_key":"Youtube","title":"Soon","live_status":"is_upcoming"},
          {"id":"lllllllllll","ie_key":"Youtube","title":"Radio","uploader":"Station","live_status":"is_live"}
        ]}"#;
        let r = parse_search(j).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].artist, "Band");
        assert_eq!(r[0].channel, "Band - Topic");
        assert_eq!(r[0].duration_ms, Some(185_000));
        assert_eq!(
            r[0].thumb,
            "https://i.ytimg.com/vi/abcdefghijk/mqdefault.jpg"
        );
        assert!(r[1].live);
        assert_eq!(r[1].url, "https://www.youtube.com/watch?v=lllllllllll");
        assert!(parse_search("not json").is_err());
    }

    #[test]
    fn friendly_errors() {
        let e = |s: &str| friendly_error(s);
        assert!(e("ERROR: [youtube] x: This video is DRM protected").contains("DRM"));
        assert!(e("ERROR: [youtube] x: Sign in to confirm you’re not a bot.").contains("bot"));
        assert!(
            e("ERROR: unable to download: HTTP Error 429: Too Many Requests")
                .contains("rate-limiting")
        );
        assert!(e("ERROR: [youtube] x: Sign in to confirm your age.").contains("age-restricted"));
        assert!(e("ERROR: [youtube] x: Video unavailable").contains("unavailable"));
        assert_eq!(
            e("ERROR: unable to download video data: HTTP Error 403: Forbidden"),
            FORBIDDEN_MSG
        );
        // "DRC" (dynamic range compression) format notes are not DRM.
        assert!(!e("[info] 251-drc webm\nERROR: Something odd").contains("DRM"));
        assert!(e("ERROR: [youtube] x: Private video. Sign in").contains("private"));
        assert!(e("ERROR: Unsupported URL: https://example.com").contains("doesn't support"));
        assert_eq!(
            e("WARNING: a\nERROR: [generic] abc: Something odd"),
            "Something odd"
        );
        assert!(
            e("ERROR: [Errno 30] Read-only file system: '/srv/music/x.webp'")
                .contains("ReadWritePaths")
        );
        assert!(e("ERROR: [Errno 13] Permission denied: '/x'").contains("permissions"));
        assert_eq!(
            e("ERROR: [Errno 99] Odd thing: '/x'"),
            "[Errno 99] Odd thing: '/x'"
        );
    }

    #[test]
    fn checksum_lookup() {
        let sums = "aa  yt-dlp\n58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a  yt-dlp_linux\n";
        assert_eq!(
            expected_sha256(sums, "yt-dlp_linux").as_deref(),
            Some("58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a")
        );
        assert_eq!(expected_sha256(sums, "yt-dlp"), None); // malformed digest
    }

    #[test]
    fn args_are_safe_and_complete() {
        let t = Tools::default();
        let o = Options {
            dest_dir: PathBuf::from("/m"),
            format: "mp3".into(),
            quality: 192,
            ..Default::default()
        };
        let a = build_args(&t, "https://youtu.be/aqz-KE-bpKQ", &o);
        let n = a.len();
        assert_eq!(a[n - 2], "--");
        assert_eq!(a[n - 1], "https://youtu.be/aqz-KE-bpKQ");
        assert!(a
            .windows(2)
            .any(|w| w[0] == "--audio-quality" && w[1] == "192K"));
        assert!(a.iter().any(|x| x == "--ignore-config"));
        assert!(a
            .windows(2)
            .any(|w| w[0] == "-o" && w[1].ends_with("[%(id)s].%(ext)s")));
        assert!(a
            .windows(2)
            .any(|w| w[0] == "-f" && w[1] == "bestaudio/best"));
        assert!(!a.iter().any(|x| x.contains("cookies")));
        let o = Options {
            format: "flac".into(),
            quality: 320,
            ..o
        };
        let a = build_args(&t, "https://youtu.be/aqz-KE-bpKQ", &o);
        assert!(!a.iter().any(|x| x == "--audio-quality"));
    }

    #[test]
    fn finds_existing_import_by_id() {
        let dir = std::env::temp_dir().join(format!("susic-ytdlp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("A - B [aqz-KE-bpKQ].mp3"), b"x").unwrap();
        std::fs::write(dir.join("A - B [aqz-KE-bpKQ].webm.part"), b"x").unwrap();
        assert!(existing_import(&dir, "aqz-KE-bpKQ").is_some());
        assert!(existing_import(&dir, "XVLVeGK16so").is_none());
        std::fs::remove_dir_all(dir).ok();
    }
}
