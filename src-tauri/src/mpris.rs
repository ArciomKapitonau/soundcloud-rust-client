//! MPRIS2 integration (media keys, desktop media widgets) via `souvlaki`.
//!
//! `MediaControls` lives on its own thread; the player sends it updates
//! through a channel, and control events are forwarded back to the player.

use crate::audio::{Player, Status};
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};
use std::sync::{mpsc, Arc, Weak};
use std::time::Duration;
use tauri::AppHandle;

pub enum MprisUpdate {
    Metadata { title: String, artist: String, cover: Option<String>, duration: Duration },
    Playback { status: Status, position: f64 },
    Volume(f64),
}

pub struct MprisHandle {
    tx: std::sync::Mutex<mpsc::Sender<MprisUpdate>>,
}

impl MprisHandle {
    pub fn send(&self, u: MprisUpdate) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(u);
        }
    }
}

pub fn start(app: AppHandle, player: &Arc<Player>) {
    let (tx, rx) = mpsc::channel::<MprisUpdate>();
    let weak: Weak<Player> = Arc::downgrade(player);
    let spawned = std::thread::Builder::new()
        .name("mpris".into())
        .spawn(move || {
            let config = PlatformConfig {
                dbus_name: "sc_rust_client",
                display_name: "SoundCloud Rust Client",
                hwnd: None,
            };
            let mut controls = match MediaControls::new(config) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("MPRIS unavailable: {e:?}");
                    return;
                }
            };
            let app2 = app.clone();
            let res = controls.attach(move |ev: MediaControlEvent| {
                let Some(p) = weak.upgrade() else { return };
                let app = app2.clone();
                tauri::async_runtime::spawn(async move {
                    match ev {
                        MediaControlEvent::Play => p.play().await,
                        MediaControlEvent::Pause => p.pause(),
                        MediaControlEvent::Toggle => p.toggle().await,
                        MediaControlEvent::Next => p.advance(true).await,
                        MediaControlEvent::Previous => p.previous().await,
                        MediaControlEvent::Stop => p.stop(),
                        MediaControlEvent::Seek(dir) => {
                            p.seek_relative(if dir == SeekDirection::Forward { 10.0 } else { -10.0 })
                        }
                        MediaControlEvent::SeekBy(dir, d) => {
                            let s = d.as_secs_f64();
                            p.seek_relative(if dir == SeekDirection::Forward { s } else { -s })
                        }
                        MediaControlEvent::SetPosition(MediaPosition(d)) => p.seek(d.as_secs_f64()),
                        MediaControlEvent::SetVolume(v) if v.is_finite() => p.set_volume(v as f32),
                        MediaControlEvent::SetVolume(_) => {}
                        MediaControlEvent::Raise => crate::tray::show_main_window(&app),
                        MediaControlEvent::Quit => app.exit(0),
                        MediaControlEvent::OpenUri(_) => {}
                    }
                });
            });
            if let Err(e) = res {
                log::warn!("MPRIS attach failed: {e:?}");
                return;
            }
            let _ = controls.set_playback(MediaPlayback::Stopped);
            while let Ok(update) = rx.recv() {
                let r = match update {
                    MprisUpdate::Metadata { title, artist, cover, duration } => controls.set_metadata(MediaMetadata {
                        title: Some(&title),
                        artist: Some(&artist),
                        album: Some("SoundCloud"),
                        cover_url: cover.as_deref(),
                        duration: Some(duration),
                    }),
                    MprisUpdate::Playback { status, position } => {
                        let progress = Some(MediaPosition(Duration::try_from_secs_f64(position.max(0.0)).unwrap_or_default()));
                        controls.set_playback(match status {
                            Status::Playing | Status::Loading => MediaPlayback::Playing { progress },
                            Status::Paused => MediaPlayback::Paused { progress },
                            Status::Idle => MediaPlayback::Stopped,
                        })
                    }
                    MprisUpdate::Volume(v) => controls.set_volume(v),
                };
                if let Err(e) = r {
                    log::debug!("MPRIS update failed: {e:?}");
                }
            }
        })
;
    match spawned {
        Ok(_) => player.set_mpris(MprisHandle { tx: std::sync::Mutex::new(tx) }),
        Err(e) => log::warn!("MPRIS disabled: cannot start thread: {e}"),
    }
}
