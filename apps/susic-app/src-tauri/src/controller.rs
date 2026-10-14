//! Application controller: owns the library, play queue and playback engine and
//! drives auto-advance in Rust, so playback continues when the UI is suspended.

use crate::engine::{Engine, EngineEvent, MediaInfo, RemoteCommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use susic_core::{PlayQueue, RepeatMode, Track};
use susic_library::Library;

pub type Emitter = Arc<dyn Fn(&str, Value) + Send + Sync>;
pub type SessionHook = Box<dyn Fn(&Track, bool) + Send>;

#[derive(Debug, Clone, Serialize, Default)]
pub struct PlayerState {
    pub track_id: Option<i64>,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub volume: f32,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct SavedQueue {
    queue: PlayQueue,
    position_ms: i64,
}

pub struct Controller {
    pub lib: Mutex<Library>,
    pub queue: Mutex<PlayQueue>,
    pub state: Mutex<PlayerState>,
    pub data_dir: PathBuf,
    pub artwork_dir: PathBuf,
    pub music_dir: PathBuf,
    pub scanning: AtomicBool,
    pub cancel_scan: AtomicBool,
    engine: OnceLock<Box<dyn Engine>>,
    emit: Emitter,
    counted: AtomicBool,
    last_save: AtomicI64,
    /// Position to seek to when the restored track is first played.
    pending_restore: Mutex<Option<i64>>,
    pub media_session: Mutex<Option<SessionHook>>,
}

impl Controller {
    pub fn new(data_dir: PathBuf, emit: Emitter) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        let artwork_dir = data_dir.join("artwork");
        let music_dir = data_dir.join("music");
        std::fs::create_dir_all(&artwork_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&music_dir).map_err(|e| e.to_string())?;
        let lib = Library::open(data_dir.join("library.db")).map_err(|e| e.to_string())?;
        lib.mark_interrupted_jobs().ok();
        let c = Arc::new(Self {
            lib: Mutex::new(lib),
            queue: Mutex::new(PlayQueue::new()),
            state: Mutex::new(PlayerState {
                volume: 1.0,
                ..Default::default()
            }),
            data_dir,
            artwork_dir,
            music_dir,
            scanning: AtomicBool::new(false),
            cancel_scan: AtomicBool::new(false),
            engine: OnceLock::new(),
            emit,
            counted: AtomicBool::new(false),
            last_save: AtomicI64::new(0),
            pending_restore: Mutex::new(None),
            media_session: Mutex::new(None),
        });
        Ok(c)
    }

    pub fn set_engine(self: &Arc<Self>, e: Box<dyn Engine>) {
        let _ = self.engine.set(e);
    }

    fn engine(&self) -> &dyn Engine {
        self.engine.get().expect("engine not set").as_ref()
    }

    pub fn emit(&self, event: &str, payload: Value) {
        (self.emit)(event, payload);
    }

    pub fn with_lib<T>(
        &self,
        f: impl FnOnce(&Library) -> susic_library::Result<T>,
    ) -> Result<T, String> {
        let l = self.lib.lock().unwrap();
        f(&l).map_err(|e| e.to_string())
    }

    // ---- engine events ----------------------------------------------------

    pub fn on_engine_event(self: &Arc<Self>, ev: EngineEvent) {
        match ev {
            EngineEvent::State {
                playing,
                position_ms,
                duration_ms,
            } => {
                let snapshot = {
                    let mut s = self.state.lock().unwrap();
                    s.playing = playing;
                    s.position_ms = position_ms;
                    if duration_ms > 0 {
                        s.duration_ms = duration_ms;
                    }
                    s.clone()
                };
                self.emit("player://state", serde_json::to_value(&snapshot).unwrap());
                if playing {
                    self.maybe_count_play(&snapshot);
                    let now = susic_library::now();
                    if now - self.last_save.load(Ordering::Relaxed) >= 5 {
                        self.last_save.store(now, Ordering::Relaxed);
                        self.save_queue();
                    }
                }
            }
            EngineEvent::Ended => {
                let next = self.queue.lock().unwrap().next(true);
                match next {
                    Some(id) => self.start_track(id, 0, true),
                    None => {
                        let mut s = self.state.lock().unwrap();
                        s.playing = false;
                        s.position_ms = 0;
                        let snap = s.clone();
                        drop(s);
                        self.emit("player://state", serde_json::to_value(&snap).unwrap());
                        self.save_queue();
                    }
                }
            }
            EngineEvent::Error(e) => {
                {
                    let mut s = self.state.lock().unwrap();
                    s.playing = false;
                    s.error = Some(e.clone());
                }
                self.emit("player://error", json!({ "message": e }));
            }
            EngineEvent::Remote(cmd) => match cmd {
                RemoteCommand::Play => self.play(),
                RemoteCommand::Pause => self.pause(),
                RemoteCommand::Toggle => self.toggle(),
                RemoteCommand::Next => self.next(),
                RemoteCommand::Prev => self.prev(),
                RemoteCommand::Stop => self.pause(),
            },
        }
    }

    fn maybe_count_play(&self, s: &PlayerState) {
        let Some(id) = s.track_id else { return };
        let threshold = if s.duration_ms > 0 {
            (s.duration_ms / 2).min(30_000)
        } else {
            30_000
        };
        if s.position_ms >= threshold
            && !self.counted.swap(true, Ordering::Relaxed)
            && self.with_lib(|l| l.mark_played(id)).is_ok()
        {
            self.emit("library://changed", json!({ "reason": "played" }));
        }
    }

    // ---- transport --------------------------------------------------------

    fn start_track(self: &Arc<Self>, id: i64, start_ms: i64, autoplay: bool) {
        let track = match self.with_lib(|l| l.get_track(id)) {
            Ok(Some(t)) => t,
            _ => {
                // Track vanished: skip it rather than stall the queue.
                let next = self.queue.lock().unwrap().next(true);
                if let Some(n) = next {
                    if n != id {
                        self.start_track(n, 0, autoplay);
                    }
                }
                return;
            }
        };
        if !std::path::Path::new(&track.path).exists() {
            self.on_engine_event(EngineEvent::Error(format!(
                "File not found: {}",
                track.path
            )));
            self.emit("queue://changed", json!({}));
            return;
        }
        self.counted.store(false, Ordering::Relaxed);
        {
            let mut s = self.state.lock().unwrap();
            s.track_id = Some(id);
            s.position_ms = start_ms;
            s.duration_ms = track.duration_ms;
            s.playing = autoplay;
            s.error = None;
        }
        self.engine().load(
            MediaInfo {
                path: track.path.clone(),
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                artwork: track.artwork.clone(),
                duration_ms: track.duration_ms,
            },
            start_ms,
            autoplay,
        );
        if let Some(f) = self.media_session.lock().unwrap().as_ref() {
            f(&track, autoplay);
        }
        self.emit("player://track", serde_json::to_value(&track).unwrap());
        self.emit("queue://changed", json!({}));
        self.save_queue();
    }

    pub fn play_tracks(self: &Arc<Self>, ids: Vec<i64>, start: usize, shuffle: Option<bool>) {
        {
            let mut q = self.queue.lock().unwrap();
            if let Some(s) = shuffle {
                q.shuffle = s;
            }
            q.set(ids, start);
        }
        *self.pending_restore.lock().unwrap() = None;
        let current = self.queue.lock().unwrap().current_id();
        if let Some(id) = current {
            self.start_track(id, 0, true);
        }
    }

    pub fn play(self: &Arc<Self>) {
        let restore = self.pending_restore.lock().unwrap().take();
        let loaded = self.state.lock().unwrap().position_ms > 0 || restore.is_none();
        if let Some(pos) = restore {
            let current = self.queue.lock().unwrap().current_id();
            if let Some(id) = current {
                self.start_track(id, pos, true);
                return;
            }
        }
        let has_track = self.state.lock().unwrap().track_id.is_some();
        if !has_track {
            let current = self.queue.lock().unwrap().current_id();
            if let Some(id) = current {
                self.start_track(id, 0, true);
            }
            return;
        }
        let _ = loaded;
        self.engine().play();
        self.state.lock().unwrap().playing = true;
        self.notify_session();
    }

    pub fn pause(&self) {
        self.engine().pause();
        self.state.lock().unwrap().playing = false;
        self.notify_session();
        self.save_queue();
    }

    pub fn toggle(self: &Arc<Self>) {
        let playing = self.state.lock().unwrap().playing;
        if playing {
            self.pause()
        } else {
            self.play()
        }
    }

    fn notify_session(&self) {
        let (id, playing) = {
            let s = self.state.lock().unwrap();
            (s.track_id, s.playing)
        };
        if let (Some(id), Some(f)) = (id, self.media_session.lock().unwrap().as_ref()) {
            if let Ok(Some(t)) = self.with_lib(|l| l.get_track(id)) {
                f(&t, playing);
            }
        }
    }

    pub fn next(self: &Arc<Self>) {
        let n = self.queue.lock().unwrap().next(false);
        match n {
            Some(id) => self.start_track(id, 0, true),
            None => {
                self.engine().stop();
                self.state.lock().unwrap().playing = false;
            }
        }
    }

    pub fn prev(self: &Arc<Self>) {
        let pos = self.state.lock().unwrap().position_ms;
        if pos > 3000 {
            self.seek(0);
            return;
        }
        let p = self.queue.lock().unwrap().prev();
        if let Some(id) = p {
            self.start_track(id, 0, true);
        }
    }

    pub fn seek(self: &Arc<Self>, ms: i64) {
        // A restored-but-not-yet-loaded track must be loaded at the seek target.
        let restore = self.pending_restore.lock().unwrap().take();
        if restore.is_some() {
            let current = self.queue.lock().unwrap().current_id();
            if let Some(id) = current {
                self.start_track(id, ms.max(0), false);
                return;
            }
        }
        self.engine().seek(ms);
        self.state.lock().unwrap().position_ms = ms;
    }

    pub fn set_volume(&self, v: f32) {
        self.engine().set_volume(v);
        self.state.lock().unwrap().volume = v;
        let _ = self.with_lib(|l| l.set_setting("volume", &v.to_string()));
    }

    pub fn jump(self: &Arc<Self>, index: usize) {
        let id = self.queue.lock().unwrap().jump_to(index);
        if let Some(id) = id {
            self.start_track(id, 0, true);
        }
    }

    pub fn remove_from_queue(self: &Arc<Self>, index: usize) {
        let (was_current, id_after) = {
            let mut q = self.queue.lock().unwrap();
            let was = q.current == Some(index);
            q.remove(index);
            (was, q.current_id())
        };
        if was_current {
            match id_after {
                Some(id) => self.start_track(id, 0, true),
                None => {
                    self.engine().stop();
                    let mut s = self.state.lock().unwrap();
                    s.playing = false;
                    s.track_id = None;
                }
            }
        }
        self.emit("queue://changed", json!({}));
        self.save_queue();
    }

    pub fn clear_queue(&self) {
        self.queue.lock().unwrap().clear();
        self.engine().stop();
        {
            let mut s = self.state.lock().unwrap();
            s.playing = false;
            s.track_id = None;
            s.position_ms = 0;
            s.duration_ms = 0;
        }
        self.emit("queue://changed", json!({}));
        self.save_queue();
    }

    // ---- persistence ------------------------------------------------------

    pub fn save_queue(&self) {
        let saved = SavedQueue {
            queue: self.queue.lock().unwrap().clone(),
            position_ms: self.state.lock().unwrap().position_ms,
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            let _ = self.with_lib(|l| l.set_setting("queue_state", &json));
        }
    }

    /// Restore queue + position at startup, paused.
    pub fn restore(self: &Arc<Self>) {
        if let Ok(Some(v)) = self.with_lib(|l| l.get_setting("volume")) {
            if let Ok(f) = v.parse::<f32>() {
                self.state.lock().unwrap().volume = f.clamp(0.0, 1.0);
                self.engine().set_volume(f);
            }
        }
        let Ok(Some(json)) = self.with_lib(|l| l.get_setting("queue_state")) else {
            return;
        };
        let Ok(saved) = serde_json::from_str::<SavedQueue>(&json) else {
            return;
        };
        // Drop ids that no longer exist.
        let ids = saved.queue.items.clone();
        let existing: Vec<i64> = self
            .with_lib(|l| l.get_tracks(&ids))
            .map(|v| v.into_iter().map(|t| t.id).collect())
            .unwrap_or_default();
        if existing.is_empty() {
            return;
        }
        let cur_id = saved
            .queue
            .current
            .and_then(|i| saved.queue.items.get(i).copied());
        let start = cur_id
            .and_then(|c| existing.iter().position(|&x| x == c))
            .unwrap_or(0);
        {
            let mut q = self.queue.lock().unwrap();
            q.shuffle = saved.queue.shuffle;
            q.repeat = saved.queue.repeat;
            q.set(existing, start);
        }
        let id = self.queue.lock().unwrap().current_id();
        if let Some(id) = id {
            if let Ok(Some(t)) = self.with_lib(|l| l.get_track(id)) {
                let mut s = self.state.lock().unwrap();
                s.track_id = Some(id);
                s.position_ms = saved.position_ms;
                s.duration_ms = t.duration_ms;
                s.playing = false;
                *self.pending_restore.lock().unwrap() = Some(saved.position_ms);
            }
        }
    }

    pub fn set_shuffle(&self, on: bool) {
        self.queue.lock().unwrap().set_shuffle(on);
        self.emit("queue://changed", json!({}));
        self.save_queue();
    }

    pub fn set_repeat(&self, r: RepeatMode) {
        self.queue.lock().unwrap().set_repeat(r);
        self.emit("queue://changed", json!({}));
        self.save_queue();
    }
}
