//! Playback engine abstraction. Desktop uses rodio/symphonia in a dedicated
//! audio thread; Android delegates to the Media3 plugin (see `engine_android`).

use std::sync::Arc;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum EngineEvent {
    /// Periodic transport state.
    State {
        playing: bool,
        position_ms: i64,
        duration_ms: i64,
    },
    /// Current media finished naturally.
    Ended,
    Error(String),
    /// Hardware / OS media controls.
    Remote(RemoteCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RemoteCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Prev,
    Stop,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MediaInfo {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: Option<String>,
    pub duration_ms: i64,
    /// File extension hint (needed when `path` is a stream URL).
    pub format: String,
}

pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

pub trait Engine: Send + Sync {
    fn load(&self, media: MediaInfo, start_ms: i64, autoplay: bool);
    fn play(&self);
    fn pause(&self);
    fn seek(&self, ms: i64);
    fn set_volume(&self, v: f32);
    fn stop(&self);
}

#[cfg(not(target_os = "android"))]
pub mod desktop {
    use super::*;
    use rodio::{Decoder, DeviceSinkBuilder, Player};
    use std::io::BufReader;
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::time::{Duration, Instant};

    enum Cmd {
        Load(MediaInfo, i64, bool),
        Play,
        Pause,
        Seek(i64),
        Volume(f32),
        Stop,
    }

    pub struct DesktopEngine {
        tx: Sender<Cmd>,
    }

    impl DesktopEngine {
        pub fn new(sink: EventSink) -> Self {
            let (tx, rx) = mpsc::channel::<Cmd>();
            std::thread::Builder::new()
                .name("susic-audio".into())
                .spawn(move || audio_thread(rx, sink))
                .expect("spawn audio thread");
            Self { tx }
        }
    }

    impl Engine for DesktopEngine {
        fn load(&self, media: MediaInfo, start_ms: i64, autoplay: bool) {
            let _ = self.tx.send(Cmd::Load(media, start_ms, autoplay));
        }
        fn play(&self) {
            let _ = self.tx.send(Cmd::Play);
        }
        fn pause(&self) {
            let _ = self.tx.send(Cmd::Pause);
        }
        fn seek(&self, ms: i64) {
            let _ = self.tx.send(Cmd::Seek(ms));
        }
        fn set_volume(&self, v: f32) {
            let _ = self.tx.send(Cmd::Volume(v));
        }
        fn stop(&self) {
            let _ = self.tx.send(Cmd::Stop);
        }
    }

    trait ReadSeek: std::io::Read + std::io::Seek + Send + Sync {}
    impl<T: std::io::Read + std::io::Seek + Send + Sync> ReadSeek for T {}

    fn open(media: &MediaInfo) -> Result<Decoder<Box<dyn ReadSeek>>, String> {
        let path = media.path.as_str();
        let (reader, len, ext): (Box<dyn ReadSeek>, u64, Option<String>) =
            if path.starts_with("http://") || path.starts_with("https://") {
                let r = susic_client::HttpRangeReader::open(path)
                    .map_err(|e| format!("Cannot reach server: {e}"))?;
                let len = r.len();
                let ext = (!media.format.is_empty()).then(|| media.format.clone());
                (Box::new(r), len, ext)
            } else {
                let f = std::fs::File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                let ext = std::path::Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(String::from);
                (Box::new(BufReader::new(f)), len, ext)
            };
        let mut b = Decoder::builder().with_data(reader).with_seekable(true);
        if len > 0 {
            b = b.with_byte_len(len);
        }
        if let Some(ext) = ext.as_deref() {
            b = b.with_hint(ext);
        }
        b.build()
            .map_err(|e| format!("Unsupported or corrupt audio: {e}"))
    }

    fn audio_thread(rx: mpsc::Receiver<Cmd>, sink: EventSink) {
        // Device is opened lazily so a missing audio device doesn't block startup
        // and can be retried on the next load.
        let mut device: Option<rodio::MixerDeviceSink> = None;
        let mut player: Option<Player> = None;
        let mut duration_ms = 0i64;
        let mut volume = 1.0f32;
        let mut loaded = false;
        let mut paused = true;
        let mut last_tick = Instant::now();

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(cmd) => match cmd {
                    Cmd::Load(media, start, autoplay) => {
                        if let Some(p) = player.take() {
                            p.stop();
                        }
                        loaded = false;
                        if device.is_none() {
                            match DeviceSinkBuilder::open_default_sink() {
                                Ok(mut d) => {
                                    d.log_on_drop(false);
                                    device = Some(d)
                                }
                                Err(e) => {
                                    sink(EngineEvent::Error(format!(
                                        "No audio output available: {e}"
                                    )));
                                    continue;
                                }
                            }
                        }
                        match open(&media) {
                            Ok(dec) => {
                                let p = Player::connect_new(device.as_ref().unwrap().mixer());
                                p.set_volume(volume);
                                p.pause();
                                p.append(dec);
                                if start > 0 {
                                    let _ = p.try_seek(Duration::from_millis(start as u64));
                                }
                                duration_ms = media.duration_ms;
                                if autoplay {
                                    p.play();
                                }
                                paused = !autoplay;
                                loaded = true;
                                player = Some(p);
                                last_tick = Instant::now() - Duration::from_secs(1);
                            }
                            Err(e) => {
                                sink(EngineEvent::Error(e));
                            }
                        }
                    }
                    Cmd::Play => {
                        if let Some(p) = &player {
                            p.play();
                            paused = false;
                            last_tick = Instant::now() - Duration::from_secs(1);
                        }
                    }
                    Cmd::Pause => {
                        if let Some(p) = &player {
                            p.pause();
                            paused = true;
                            last_tick = Instant::now() - Duration::from_secs(1);
                        }
                    }
                    Cmd::Seek(ms) => {
                        if let Some(p) = &player {
                            let _ = p.try_seek(Duration::from_millis(ms.max(0) as u64));
                            last_tick = Instant::now() - Duration::from_secs(1);
                        }
                    }
                    Cmd::Volume(v) => {
                        volume = v.clamp(0.0, 1.0);
                        if let Some(p) = &player {
                            p.set_volume(volume);
                        }
                    }
                    Cmd::Stop => {
                        if let Some(p) = player.take() {
                            p.stop();
                        }
                        loaded = false;
                        paused = true;
                    }
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }

            if let Some(p) = &player {
                if loaded && !paused && p.empty() {
                    loaded = false;
                    paused = true;
                    sink(EngineEvent::Ended);
                    continue;
                }
                if last_tick.elapsed() >= Duration::from_millis(250) {
                    last_tick = Instant::now();
                    sink(EngineEvent::State {
                        playing: loaded && !paused,
                        position_ms: p.get_pos().as_millis() as i64,
                        duration_ms,
                    });
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
pub mod android {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::time::Duration;
    use tauri::plugin::PluginHandle;
    use tauri::Wry;

    enum Cmd {
        Load(MediaInfo, i64, bool),
        Play,
        Pause,
        Seek(i64),
        Volume(f32),
        Stop,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LoadArgs {
        path: String,
        title: String,
        artist: String,
        album: String,
        artwork: Option<String>,
        start_ms: i64,
        autoplay: bool,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct StateResp {
        playing: bool,
        position_ms: i64,
        duration_ms: i64,
        loaded: bool,
        #[serde(default)]
        events: Vec<String>,
    }

    /// Talks to the Kotlin Media3 plugin from a dedicated thread so the UI/main
    /// thread is never blocked on a JNI round-trip.
    pub struct AndroidEngine {
        tx: Sender<Cmd>,
    }

    impl AndroidEngine {
        pub fn new(handle: PluginHandle<Wry>, sink: EventSink) -> Self {
            let (tx, rx) = mpsc::channel::<Cmd>();
            std::thread::Builder::new()
                .name("susic-media3".into())
                .spawn(move || {
                    let mut active = false;
                    let mut duration = 0i64;
                    loop {
                        let wait = if active { 250 } else { 1000 };
                        match rx.recv_timeout(Duration::from_millis(wait)) {
                            Ok(cmd) => {
                                let r: Result<(), _> = match cmd {
                                    Cmd::Load(m, start, autoplay) => {
                                        duration = m.duration_ms;
                                        active = true;
                                        handle.run_mobile_plugin(
                                            "load",
                                            LoadArgs {
                                                path: m.path,
                                                title: m.title,
                                                artist: m.artist,
                                                album: m.album,
                                                artwork: m.artwork,
                                                start_ms: start,
                                                autoplay,
                                            },
                                        )
                                    }
                                    Cmd::Play => handle.run_mobile_plugin("play", ()),
                                    Cmd::Pause => handle.run_mobile_plugin("pause", ()),
                                    Cmd::Seek(ms) => handle.run_mobile_plugin(
                                        "seek",
                                        serde_json::json!({ "positionMs": ms }),
                                    ),
                                    Cmd::Volume(v) => handle.run_mobile_plugin(
                                        "setVolume",
                                        serde_json::json!({ "volume": v }),
                                    ),
                                    Cmd::Stop => {
                                        active = false;
                                        handle.run_mobile_plugin("stop", ())
                                    }
                                };
                                if let Err(e) = r {
                                    sink(EngineEvent::Error(format!("Playback error: {e}")));
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => {}
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                        if !active {
                            // Still drain remote-control events (e.g. notification buttons).
                        }
                        if let Ok(st) = handle.run_mobile_plugin::<StateResp>("state", ()) {
                            let mut ended = false;
                            for ev in st.events {
                                match ev.as_str() {
                                    "ended" => ended = true,
                                    "next" => sink(EngineEvent::Remote(RemoteCommand::Next)),
                                    "prev" => sink(EngineEvent::Remote(RemoteCommand::Prev)),
                                    e if e.starts_with("error:") => {
                                        sink(EngineEvent::Error(e[6..].to_string()))
                                    }
                                    _ => {}
                                }
                            }
                            if ended {
                                sink(EngineEvent::Ended);
                            } else if active && st.loaded {
                                sink(EngineEvent::State {
                                    playing: st.playing,
                                    position_ms: st.position_ms,
                                    duration_ms: if st.duration_ms > 0 {
                                        st.duration_ms
                                    } else {
                                        duration
                                    },
                                });
                            }
                        }
                    }
                })
                .expect("spawn media3 thread");
            Self { tx }
        }
    }

    impl Engine for AndroidEngine {
        fn load(&self, media: MediaInfo, start_ms: i64, autoplay: bool) {
            let _ = self.tx.send(Cmd::Load(media, start_ms, autoplay));
        }
        fn play(&self) {
            let _ = self.tx.send(Cmd::Play);
        }
        fn pause(&self) {
            let _ = self.tx.send(Cmd::Pause);
        }
        fn seek(&self, ms: i64) {
            let _ = self.tx.send(Cmd::Seek(ms));
        }
        fn set_volume(&self, v: f32) {
            let _ = self.tx.send(Cmd::Volume(v));
        }
        fn stop(&self) {
            let _ = self.tx.send(Cmd::Stop);
        }
    }
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::desktop::DesktopEngine;
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    fn write_wav(path: &std::path::Path, secs: f32) {
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
            let v = (((i as f32) * 0.1).sin() * 6000.0) as i16;
            d.extend(v.to_le_bytes());
        }
        std::fs::write(path, d).unwrap();
    }

    /// Requires a working audio output device, so it is opt-in:
    /// `cargo test -p susic-app -- --ignored desktop_engine`.
    #[test]
    #[ignore]
    fn desktop_engine_plays_seeks_pauses_and_ends() {
        let dir = std::env::temp_dir().join(format!("susic-eng-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("t.wav");
        write_wav(&wav, 3.0);
        let events = Arc::new(Mutex::new(Vec::<EngineEvent>::new()));
        let ev = events.clone();
        let eng = DesktopEngine::new(Arc::new(move |e| ev.lock().unwrap().push(e)));
        let media = MediaInfo {
            path: wav.to_string_lossy().into(),
            duration_ms: 3000,
            ..Default::default()
        };
        eng.load(media, 0, true);
        let wait = |cond: &dyn Fn(&[EngineEvent]) -> bool, secs: u64| {
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(secs) {
                if cond(&events.lock().unwrap()) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        };
        let pos = |e: &EngineEvent| match e {
            EngineEvent::State {
                position_ms,
                playing: true,
                ..
            } => Some(*position_ms),
            _ => None,
        };
        assert!(
            wait(&|e| e.iter().filter_map(pos).any(|p| p > 400), 5),
            "position should advance: {:?}",
            events.lock().unwrap().last()
        );
        eng.pause();
        assert!(
            wait(
                &|e| matches!(e.last(), Some(EngineEvent::State { playing: false, .. })),
                3
            ),
            "pause"
        );
        eng.seek(2500);
        eng.play();
        assert!(
            wait(&|e| e.iter().any(|x| matches!(x, EngineEvent::Ended)), 6),
            "should end"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
