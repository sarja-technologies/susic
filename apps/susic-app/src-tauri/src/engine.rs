//! Playback engine abstraction. Desktop uses rodio/symphonia in a dedicated
//! audio thread; Android delegates to the Media3 plugin (see `engine_android`).

use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// Periodic transport state.
    State { playing: bool, position_ms: i64, duration_ms: i64 },
    /// Current media finished naturally.
    Ended,
    Error(String),
    /// Hardware / OS media controls.
    Remote(RemoteCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Prev,
    Stop,
}

#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: Option<String>,
    pub duration_ms: i64,
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

    fn open(path: &str) -> Result<Decoder<BufReader<std::fs::File>>, String> {
        let f = std::fs::File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let mut b = Decoder::builder().with_data(BufReader::new(f)).with_seekable(true);
        if len > 0 {
            b = b.with_byte_len(len);
        }
        if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
            b = b.with_hint(ext);
        }
        b.build().map_err(|e| format!("Unsupported or corrupt audio: {e}"))
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
                                    sink(EngineEvent::Error(format!("No audio output available: {e}")));
                                    continue;
                                }
                            }
                        }
                        match open(&media.path) {
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

/// Placeholder until the Android Media3 plugin is attached.
pub struct NullEngine;
impl Engine for NullEngine {
    fn load(&self, _: MediaInfo, _: i64, _: bool) {}
    fn play(&self) {}
    fn pause(&self) {}
    fn seek(&self, _: i64) {}
    fn set_volume(&self, _: f32) {}
    fn stop(&self) {}
}
