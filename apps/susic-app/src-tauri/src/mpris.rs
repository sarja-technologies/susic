//! Linux media keys / MPRIS (and the equivalent on other desktops) via souvlaki.

use crate::controller::Controller;
use crate::engine::{EngineEvent, EventSink, RemoteCommand};
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use std::sync::{Arc, Mutex};

pub fn attach(ctl: &Arc<Controller>, sink: EventSink) {
    let cfg = PlatformConfig {
        dbus_name: "susic",
        display_name: "Susic",
        hwnd: None,
    };
    let Ok(mut controls) = MediaControls::new(cfg) else {
        return;
    };
    let attached = controls.attach(move |ev| {
        let cmd = match ev {
            MediaControlEvent::Play => RemoteCommand::Play,
            MediaControlEvent::Pause => RemoteCommand::Pause,
            MediaControlEvent::Toggle => RemoteCommand::Toggle,
            MediaControlEvent::Next => RemoteCommand::Next,
            MediaControlEvent::Previous => RemoteCommand::Prev,
            MediaControlEvent::Stop => RemoteCommand::Stop,
            _ => return,
        };
        sink(EngineEvent::Remote(cmd));
    });
    if attached.is_err() {
        return;
    }
    let controls = Mutex::new(controls);
    *ctl.media_session.lock().unwrap() = Some(Box::new(move |t, playing| {
        if let Ok(mut c) = controls.lock() {
            let cover = t.artwork.as_ref().map(|a| format!("file://{a}"));
            let _ = c.set_metadata(MediaMetadata {
                title: Some(&t.title),
                artist: Some(&t.artist),
                album: Some(&t.album),
                cover_url: cover.as_deref(),
                duration: Some(std::time::Duration::from_millis(t.duration_ms.max(0) as u64)),
            });
            let _ = c.set_playback(if playing {
                MediaPlayback::Playing { progress: None }
            } else {
                MediaPlayback::Paused { progress: None }
            });
        }
    }));
}
