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
    /// Paired Susic server, if any (see `server.rs`).
    pub server: Mutex<Option<susic_client::Server>>,
    pub server_online: AtomicBool,
    /// Bumped on every explicit load so stale outage-retries cancel themselves.
    load_gen: std::sync::atomic::AtomicU64,
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
            server: Mutex::new(None),
            server_online: AtomicBool::new(false),
            load_gen: std::sync::atomic::AtomicU64::new(0),
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
                let (resume_id, pos, was_playing) = {
                    let mut s = self.state.lock().unwrap();
                    let r = (s.track_id, s.position_ms, s.playing);
                    s.playing = false;
                    s.error = Some(e.clone());
                    r
                };
                self.emit("player://error", json!({ "message": e }));
                if let Some(id) = resume_id {
                    self.schedule_stream_resume(id, pos, was_playing, &e);
                }
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

    /// A streamed track failed (server restarted, Wi-Fi dropped, VPN flapped).
    /// Poll the server for up to two minutes and resume from the same position.
    fn schedule_stream_resume(self: &Arc<Self>, id: i64, pos: i64, was_playing: bool, err: &str) {
        let streamed = self
            .with_lib(|l| l.get_track(id))
            .ok()
            .flatten()
            .map(|t| {
                t.remote_id.is_some()
                    && !matches!(self.resolve_playable(&t), Ok(p) if !p.starts_with("http"))
            })
            .unwrap_or(false);
        if !streamed
            || !(err.contains("reach server") || err.contains("onnection") || err.contains("HTTP"))
        {
            return;
        }
        let gen = self.load_gen.load(Ordering::SeqCst);
        let me = self.clone();
        std::thread::spawn(move || {
            for _ in 0..40 {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if me.load_gen.load(Ordering::SeqCst) != gen {
                    return; // user moved on
                }
                let base = me.server.lock().unwrap().as_ref().map(|s| s.base.clone());
                let Some(base) = base else { return };
                if susic_client::health(&base).is_ok() {
                    me.server_online.store(true, Ordering::Relaxed);
                    me.emit("server://state", json!({ "online": true }));
                    me.start_track(id, pos, was_playing);
                    return;
                }
                me.server_online.store(false, Ordering::Relaxed);
                me.emit("server://state", json!({ "online": false }));
            }
        });
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
        let play_path = match self.resolve_playable(&track) {
            Ok(p) => p,
            Err(msg) => {
                self.on_engine_event(EngineEvent::Error(msg));
                self.emit("queue://changed", json!({}));
                return;
            }
        };
        self.load_gen.fetch_add(1, Ordering::SeqCst);
        if play_path.starts_with("http") && track.remote_id.is_some() {
            crate::server::maybe_auto_cache(self, id);
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
                path: play_path,
                format: track.format.clone(),
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

    /// Decide where audio comes from. Preference order: a file on this device,
    /// an offline copy of a server track, then a stream from the server.
    pub fn resolve_playable(&self, t: &Track) -> Result<String, String> {
        let exists = |p: &str| p.starts_with("content://") || std::path::Path::new(p).exists();
        if t.source != "server" && exists(&t.path) {
            return Ok(t.path.clone());
        }
        if let Some(c) = &t.cache_path {
            if exists(c) {
                return Ok(c.clone());
            }
        }
        if let Some(rid) = t.remote_id {
            if let Some(srv) = self.server.lock().unwrap().as_ref() {
                return Ok(srv.stream_url(rid));
            }
            return Err("This song is only on your Susic server, which isn't connected".into());
        }
        Err(format!("File not found: {}", t.path))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, EngineEvent, MediaInfo};
    use std::sync::atomic::AtomicUsize;
    use susic_core::NewTrack;

    #[derive(Default)]
    struct Fake {
        loads: AtomicUsize,
    }
    impl Engine for Arc<Fake> {
        fn load(&self, _: MediaInfo, _: i64, _: bool) {
            self.loads.fetch_add(1, Ordering::SeqCst);
        }
        fn play(&self) {}
        fn pause(&self) {}
        fn seek(&self, _: i64) {}
        fn set_volume(&self, _: f32) {}
        fn stop(&self) {}
    }

    fn setup() -> (Arc<Controller>, Arc<Fake>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "susic-ctl-{}-{}",
            std::process::id(),
            susic_library::now()
        ));
        let c = Controller::new(dir.clone(), Arc::new(|_, _| {})).unwrap();
        let fake = Arc::new(Fake::default());
        c.set_engine(Box::new(fake.clone()));
        for i in 0..3 {
            let f = dir.join(format!("{i}.mp3"));
            std::fs::write(&f, b"x").unwrap();
            c.with_lib(|l| {
                l.upsert_track(&NewTrack {
                    title: format!("T{i}"),
                    path: f.to_string_lossy().into(),
                    hash: format!("h{i}"),
                    duration_ms: 60_000,
                    ..Default::default()
                })
            })
            .unwrap();
        }
        (c, fake, dir)
    }

    /// Regression: starting a track used to self-deadlock on the queue mutex.
    #[test]
    fn play_next_ended_and_restore_do_not_deadlock() {
        let (c, fake, dir) = setup();
        let ids: Vec<i64> = c
            .with_lib(|l| l.list_tracks(&Default::default()))
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let c2 = c.clone();
        std::thread::spawn(move || {
            c2.play_tracks(ids, 0, None);
            c2.on_engine_event(EngineEvent::State {
                playing: true,
                position_ms: 40_000,
                duration_ms: 60_000,
            });
            c2.next();
            c2.on_engine_event(EngineEvent::Ended);
            c2.prev();
            c2.seek(1000);
            c2.pause();
            c2.play();
            c2.restore();
            tx.send(()).unwrap();
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("controller deadlocked");
        assert!(fake.loads.load(Ordering::SeqCst) >= 3);
        let t = c.with_lib(|l| l.get_track(1)).unwrap().unwrap();
        assert_eq!(
            t.play_count, 1,
            "play counted once after passing the threshold"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
