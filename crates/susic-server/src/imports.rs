//! Server-side YouTube search and import via yt-dlp (opt-in: `ytdlp_enabled`).
//!
//! Paired devices — including Android, which cannot run yt-dlp itself — can
//! search and ask the server to import audio the user has the right to
//! download. Imports land in `music_dir/imports_dir`, are indexed by a normal
//! scan, and then sync to every client like any other server track. Jobs run
//! one at a time to stay gentle with the source.

use crate::api::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use susic_import::ytdlp::{self, Event, Options, Tools};

const KEEP_FINISHED: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: u64,
    pub url: String,
    pub title: String,
    /// queued | running | done | failed | cancelled
    pub status: String,
    pub progress: f64,
    pub message: String,
    /// Licence declared by the source, when it reports one.
    pub license: Option<String>,
    /// Server track id once indexed.
    pub track_id: Option<i64>,
    pub created_at: i64,
    #[serde(skip)]
    format: String,
    #[serde(skip)]
    quality: u32,
}

#[derive(Debug, Deserialize)]
pub struct NewJob {
    pub url: String,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub quality: u32,
    #[serde(default)]
    pub title: String,
}
fn default_format() -> String {
    "m4a".into()
}

#[derive(Default)]
pub struct Imports {
    jobs: Mutex<Vec<Job>>,
    cancels: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    next: AtomicU64,
    queue: Mutex<Option<mpsc::Sender<u64>>>,
    /// Serialises searches so a burst of typing can't fan out into many yt-dlp processes.
    pub search_lock: Mutex<()>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub enabled: bool,
    pub ytdlp: bool,
    pub version: Option<String>,
    pub managed: bool,
    pub ffmpeg: bool,
    pub js_runtime: Option<String>,
    pub imports_dir: String,
}

pub fn tools(st: &AppState) -> Tools {
    Tools {
        // Under systemd hardening the service user's home is read-only.
        cache_dir: Some(st.cfg.data_dir.join("cache/yt-dlp")),
        ..Tools::locate(Some(&st.cfg.tools_dir()))
    }
}

pub fn capabilities(st: &AppState) -> Capabilities {
    let s = ytdlp::tool_status(Some(&st.cfg.tools_dir()));
    Capabilities {
        enabled: st.cfg.ytdlp_enabled,
        ytdlp: s.ytdlp.is_some(),
        version: s.version,
        managed: s.managed,
        ffmpeg: s.ffmpeg.is_some(),
        js_runtime: s.js_runtime,
        imports_dir: st.cfg.imports_dir.clone(),
    }
}

/// Why yt-dlp features are unavailable, if they are.
pub fn unavailable(st: &AppState, need_ffmpeg: bool) -> Option<String> {
    if !st.cfg.ytdlp_enabled {
        return Some(
            "YouTube search and import are turned off on this server (set ytdlp_enabled = true in its config)."
                .into(),
        );
    }
    let t = tools(st);
    if t.ytdlp.is_none() {
        return Some(
            "yt-dlp is not installed on the server. Run `susic-server ytdlp-install` there.".into(),
        );
    }
    if need_ffmpeg && t.ffmpeg.is_none() {
        return Some(
            "FFmpeg is not installed on the server; it is needed to extract audio.".into(),
        );
    }
    None
}

impl Imports {
    pub fn list(&self) -> Vec<Job> {
        let mut v = self.jobs.lock().unwrap().clone();
        v.reverse();
        v
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut Job)) {
        if let Some(j) = self.jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
            f(j);
        }
    }

    fn get(&self, id: u64) -> Option<Job> {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.id == id)
            .cloned()
    }

    /// Cancel an active job, or forget a finished one. Returns false if unknown.
    pub fn cancel_or_remove(&self, id: u64) -> bool {
        if let Some(c) = self.cancels.lock().unwrap().get(&id) {
            c.store(true, Ordering::SeqCst);
            return true;
        }
        let mut jobs = self.jobs.lock().unwrap();
        let before = jobs.len();
        jobs.retain(|j| j.id != id);
        jobs.len() != before
    }
}

/// Queue an import. Validates input up front so clients get immediate errors.
pub fn submit(st: &Arc<AppState>, req: NewJob) -> Result<Job, String> {
    if let Some(why) = unavailable(st, true) {
        return Err(why);
    }
    ytdlp::validate_url(&req.url)?;
    if !ytdlp::FORMATS.contains(&req.format.as_str()) {
        return Err(format!("unsupported format '{}'", req.format));
    }
    let im = &st.imports;
    let url = req.url.trim().to_string();
    {
        let jobs = im.jobs.lock().unwrap();
        if let Some(j) = jobs
            .iter()
            .find(|j| j.url == url && (j.status == "queued" || j.status == "running"))
        {
            return Ok(j.clone());
        }
    }
    let id = im.next.fetch_add(1, Ordering::SeqCst) + 1;
    let job = Job {
        id,
        url,
        title: req.title.trim().chars().take(300).collect(),
        status: "queued".into(),
        progress: 0.0,
        message: "Waiting…".into(),
        license: None,
        track_id: None,
        created_at: crate::db::now(),
        format: req.format,
        quality: req.quality.min(512),
    };
    {
        let mut jobs = im.jobs.lock().unwrap();
        jobs.push(job.clone());
        // Bound memory: drop the oldest finished jobs.
        let finished = jobs.iter().filter(|j| !is_active(j)).count();
        if finished > KEEP_FINISHED {
            let mut drop = finished - KEEP_FINISHED;
            jobs.retain(|j| {
                if drop > 0 && !is_active(j) {
                    drop -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }
    im.cancels
        .lock()
        .unwrap()
        .insert(id, Arc::new(AtomicBool::new(false)));
    let mut q = im.queue.lock().unwrap();
    if q.is_none() {
        let (tx, rx) = mpsc::channel::<u64>();
        let st2 = st.clone();
        std::thread::Builder::new()
            .name("susic-imports".into())
            .spawn(move || {
                for id in rx {
                    process(&st2, id);
                }
            })
            .map_err(|e| e.to_string())?;
        *q = Some(tx);
    }
    q.as_ref()
        .unwrap()
        .send(id)
        .map_err(|_| "import worker stopped".to_string())?;
    Ok(job)
}

fn is_active(j: &Job) -> bool {
    j.status == "queued" || j.status == "running"
}

fn process(st: &Arc<AppState>, id: u64) {
    let im = &st.imports;
    let Some(job) = im.get(id) else { return };
    let cancel = im
        .cancels
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .unwrap_or_default();
    let finish = |status: &str, msg: String| {
        im.update(id, |j| {
            j.status = status.into();
            j.message = msg;
            if status == "done" {
                j.progress = 1.0;
            }
        });
        im.cancels.lock().unwrap().remove(&id);
    };
    if cancel.load(Ordering::SeqCst) {
        return finish("cancelled", "Cancelled".into());
    }
    im.update(id, |j| {
        j.status = "running".into();
        j.message = "Starting…".into();
    });
    let dest = st.cfg.imports_path();
    let opts = Options {
        dest_dir: dest.clone(),
        format: job.format.clone(),
        quality: job.quality,
    };
    let res = ytdlp::run(
        &tools(st),
        &job.url,
        &opts,
        cancel.clone(),
        &mut |ev| match ev {
            Event::Progress(p, m) => im.update(id, |j| {
                j.progress = p;
                j.message = m;
            }),
            Event::Info(i) => im.update(id, |j| {
                if !i.title.is_empty() {
                    j.title = i.title.clone();
                }
                j.license = i.license.clone().filter(|l| !l.is_empty());
            }),
            Event::File(_) => {}
        },
    );
    let files = match res {
        Ok(d) => d.files,
        Err(e) if e == "cancelled" => return finish("cancelled", "Cancelled".into()),
        Err(e) => {
            crate::log("warn", &format!("import #{id} failed: {e}"));
            return finish("failed", e);
        }
    };
    im.update(id, |j| {
        j.progress = 0.97;
        j.message = "Adding to the server library…".into();
    });
    // Index it. If a scan is already running it may have missed the new file,
    // so wait for it and run our own.
    loop {
        match st.run_scan() {
            Some(_) => break,
            None => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    let track_id = files.first().and_then(|f| track_for(st, f));
    im.update(id, |j| j.track_id = track_id);
    crate::log("info", &format!("import #{id} done: {}", job.url));
    finish(
        "done",
        if track_id.is_some() {
            "Added to the server library".into()
        } else {
            "Downloaded, but the server couldn't index the file".into()
        },
    );
}

fn track_for(st: &AppState, file: &Path) -> Option<i64> {
    let rel = file
        .strip_prefix(&st.cfg.music_dir)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    st.db
        .path_index()
        .ok()?
        .into_iter()
        .find(|e| e.0 == rel && !e.4)
        .map(|e| e.1)
}
