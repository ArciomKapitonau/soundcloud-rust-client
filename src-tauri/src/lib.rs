//! SoundCloud desktop client for Linux — Tauri v2 backend.

mod api;
mod audio;
mod auth;
mod commands;
mod config;
#[cfg(unix)]
mod discord;
mod models;
mod mpris;
mod stream;
mod theme;
mod tray;

use std::sync::Arc;
use tauri::{Emitter, Manager, WindowEvent};

pub struct AppState {
    pub api: Arc<api::ApiClient>,
    pub player: Arc<audio::Player>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("sc_rust_client_lib=info")).init();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main_window(app)))
        .setup(|app| {
            if let Err(e) = config::secure_existing(&config::config_dir()) {
                log::warn!("cannot tighten config permissions: {e}");
            }
            let api = Arc::new(api::ApiClient::new()?);
            let handle = app.handle().clone();
            let on_state: audio::StateSink = Arc::new(move |st: &audio::PlayerState| {
                let _ = handle.emit("player://state", st);
            });
            let player = audio::Player::new(api.clone(), on_state, audio::AudioBackend::Device);
            #[cfg(unix)]
            discord::start(&player);
            mpris::start(app.handle().clone(), &player);
            app.manage(AppState { api, player });
            tray::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.app_handle().state::<AppState>();
                api.prevent_close();
                if state.player.settings().close_to_tray {
                    // Keep playing in the background; the tray restores it.
                    let _ = window.hide();
                } else {
                    // The tray icon would otherwise keep a window-less app alive.
                    state.player.stop();
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth_status,
            commands::login_official,
            commands::set_web_credentials,
            commands::continue_anonymous,
            commands::refresh_client_id,
            commands::logout,
            commands::get_feed,
            commands::search_tracks,
            commands::get_likes,
            commands::load_next,
            commands::get_playlists,
            commands::get_playlist_tracks,
            commands::get_liked_ids,
            commands::set_like,
            commands::get_waveform,
            commands::open_url,
            commands::play_tracks,
            commands::toggle_play,
            commands::next_track,
            commands::prev_track,
            commands::jump_to,
            commands::seek,
            commands::set_volume,
            commands::set_repeat,
            commands::player_state,
            commands::get_queue,
            commands::list_audio_devices,
            commands::set_audio_device,
            commands::get_settings,
            commands::set_close_to_tray,
            commands::get_theme,
            commands::save_theme,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        log::error!("application error: {e}");
        std::process::exit(1);
    }
}
