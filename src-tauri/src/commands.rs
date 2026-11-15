//! Tauri commands exposed to the frontend.

use crate::api::Mode;
use crate::audio::{self, PlayerState};
use crate::config::{RepeatMode, Settings};
use crate::models::{Page, Playlist, Track, User};
use crate::theme::{self, ThemeConfig};
use crate::{auth, AppState};
use serde::Serialize;
use tauri::State;

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn mask(s: &Option<String>) -> Option<String> {
    s.as_deref().map(crate::config::mask)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub mode: Mode,
    pub onboarded: bool,
    pub user: Option<User>,
    pub app_client_id: Option<String>,
    pub has_app_secret: bool,
    pub v2_client_id: Option<String>,
    pub v2_oauth_token: Option<String>,
    pub scraped_client_id: Option<String>,
    pub official_expires_at: Option<i64>,
    pub user_error: Option<String>,
}

async fn status(state: &AppState) -> AuthStatus {
    let s = state.api.store();
    let (user, user_error) = match state.api.me().await {
        Ok(u) => (u, None),
        Err(e) => (None, Some(format!("{e:#}"))),
    };
    AuthStatus {
        mode: state.api.mode(),
        onboarded: s.onboarded,
        user,
        app_client_id: s.app_client_id.clone(),
        has_app_secret: s.app_client_secret.as_deref().map(|x| !x.is_empty()).unwrap_or(false),
        v2_client_id: s.v2_client_id.clone(),
        v2_oauth_token: mask(&s.v2_oauth_token),
        scraped_client_id: mask(&s.scraped_client_id),
        official_expires_at: s.official.as_ref().and_then(|o| o.expires_at),
        user_error,
    }
}

// ---------------------------------------------------------------- auth

#[tauri::command]
pub async fn auth_status(state: State<'_, AppState>) -> CmdResult<AuthStatus> {
    Ok(status(&state).await)
}

#[tauri::command]
pub async fn login_official(state: State<'_, AppState>, client_id: String, client_secret: String) -> CmdResult<AuthStatus> {
    auth::login(&state.api, client_id, client_secret).await.map_err(err)?;
    Ok(status(&state).await)
}

/// Fallback mode: credentials copied from the SoundCloud web app.
#[tauri::command]
pub async fn set_web_credentials(
    state: State<'_, AppState>,
    client_id: Option<String>,
    oauth_token: Option<String>,
) -> CmdResult<AuthStatus> {
    let clean = |s: Option<String>| {
        s.map(|v| v.trim().trim_start_matches("OAuth ").trim().to_string()).filter(|v| !v.is_empty())
    };
    let client_id = clean(client_id);
    let oauth_token = clean(oauth_token);
    state
        .api
        .update_store(|s| {
            s.v2_client_id = client_id;
            s.v2_oauth_token = oauth_token.clone();
            if oauth_token.is_some() {
                s.official = None; // web token takes over
            }
            s.onboarded = true;
        })
        .map_err(err)?;
    state.api.clear_session_cache();
    let st = status(&state).await;
    if let Some(e) = &st.user_error {
        return Err(format!("Данные сохранены, но проверка не прошла: {e}"));
    }
    Ok(st)
}

#[tauri::command]
pub async fn continue_anonymous(state: State<'_, AppState>) -> CmdResult<AuthStatus> {
    state.api.client_id().await.map_err(err)?;
    state.api.update_store(|s| s.onboarded = true).map_err(err)?;
    Ok(status(&state).await)
}

#[tauri::command]
pub async fn refresh_client_id(state: State<'_, AppState>) -> CmdResult<String> {
    let id = state.api.scrape_client_id(true).await.map_err(err)?;
    Ok(crate::config::mask(&id))
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> CmdResult<AuthStatus> {
    state
        .api
        .update_store(|s| {
            s.official = None;
            s.v2_oauth_token = None;
            s.onboarded = false;
        })
        .map_err(err)?;
    state.api.clear_session_cache();
    Ok(status(&state).await)
}

// ---------------------------------------------------------------- browse

#[tauri::command]
pub async fn get_feed(state: State<'_, AppState>) -> CmdResult<Page> {
    state.api.feed().await.map_err(err)
}

#[tauri::command]
pub async fn search_tracks(state: State<'_, AppState>, query: String) -> CmdResult<Page> {
    if query.trim().is_empty() {
        return Ok(Page::default());
    }
    state.api.search(query.trim()).await.map_err(err)
}

#[tauri::command]
pub async fn get_likes(state: State<'_, AppState>) -> CmdResult<Page> {
    state.api.likes().await.map_err(err)
}

#[tauri::command]
pub async fn load_next(state: State<'_, AppState>, next_href: String) -> CmdResult<Page> {
    state.api.next_page(&next_href).await.map_err(err)
}

#[tauri::command]
pub async fn get_playlists(state: State<'_, AppState>) -> CmdResult<Vec<Playlist>> {
    state.api.playlists().await.map_err(err)
}

#[tauri::command]
pub async fn get_playlist_tracks(state: State<'_, AppState>, playlist_id: u64) -> CmdResult<Vec<Track>> {
    state.api.playlist_tracks(playlist_id).await.map_err(err)
}

#[tauri::command]
pub async fn get_liked_ids(state: State<'_, AppState>) -> CmdResult<Vec<u64>> {
    state.api.liked_ids().await.map_err(err)
}

#[tauri::command]
pub async fn set_like(state: State<'_, AppState>, track_id: u64, like: bool) -> CmdResult<bool> {
    state.api.set_like(track_id, like).await.map_err(err)?;
    state.player.emit();
    Ok(like)
}

#[tauri::command]
pub async fn get_waveform(state: State<'_, AppState>, url: String, bars: Option<usize>) -> CmdResult<Vec<f32>> {
    if url.len() > 2048 {
        return Err("слишком длинный URL".into());
    }
    state.api.waveform(&url, bars.unwrap_or(180)).await.map_err(err)
}

#[tauri::command]
pub fn open_url(url: String) -> CmdResult<()> {
    if !is_soundcloud_page(&url) {
        return Err("можно открывать только страницы soundcloud.com".into());
    }
    open::that_detached(url).map_err(|e| e.to_string())
}

/// Only https links to soundcloud.com (track/user pages, app registration).
fn is_soundcloud_page(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("");
            u.scheme() == "https"
                && u.username().is_empty()
                && u.password().is_none()
                && (host == "soundcloud.com" || host.ends_with(".soundcloud.com"))
        }
        Err(_) => false,
    }
}

// ---------------------------------------------------------------- player

#[tauri::command]
pub async fn play_tracks(
    state: State<'_, AppState>,
    tracks: Vec<Track>,
    index: usize,
    start_fraction: Option<f64>,
) -> CmdResult<()> {
    let player = state.player.clone();
    // Don't block the IPC call on network/decoder start-up.
    tauri::async_runtime::spawn(async move { player.play_queue(tracks, index, start_fraction).await });
    Ok(())
}

#[tauri::command]
pub async fn toggle_play(state: State<'_, AppState>) -> CmdResult<()> {
    let p = state.player.clone();
    tauri::async_runtime::spawn(async move { p.toggle().await });
    Ok(())
}

#[tauri::command]
pub async fn next_track(state: State<'_, AppState>) -> CmdResult<()> {
    let p = state.player.clone();
    tauri::async_runtime::spawn(async move { p.advance(true).await });
    Ok(())
}

#[tauri::command]
pub async fn prev_track(state: State<'_, AppState>) -> CmdResult<()> {
    let p = state.player.clone();
    tauri::async_runtime::spawn(async move { p.previous().await });
    Ok(())
}

#[tauri::command]
pub async fn jump_to(state: State<'_, AppState>, index: usize) -> CmdResult<()> {
    let p = state.player.clone();
    tauri::async_runtime::spawn(async move { p.jump_to(index).await });
    Ok(())
}

#[tauri::command]
pub fn seek(state: State<'_, AppState>, seconds: f64) {
    state.player.seek(seconds);
}

#[tauri::command]
pub fn set_volume(state: State<'_, AppState>, volume: f32) {
    state.player.set_volume(volume);
}

#[tauri::command]
pub fn set_repeat(state: State<'_, AppState>, mode: RepeatMode) {
    state.player.set_repeat(mode);
}

#[tauri::command]
pub fn player_state(state: State<'_, AppState>) -> PlayerState {
    state.player.state()
}

#[tauri::command]
pub fn get_queue(state: State<'_, AppState>) -> Vec<Track> {
    state.player.queue()
}

#[tauri::command]
pub fn list_audio_devices() -> Vec<String> {
    audio::list_output_devices()
}

#[tauri::command]
pub fn set_audio_device(state: State<'_, AppState>, name: Option<String>) {
    state.player.set_device(name.filter(|n| !n.is_empty()));
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.player.settings()
}

#[tauri::command]
pub fn set_close_to_tray(state: State<'_, AppState>, enabled: bool) {
    state.player.set_close_to_tray(enabled);
}

// ---------------------------------------------------------------- theme

/// Selected preset + custom CSS, read at startup before the first render.
#[tauri::command]
pub fn get_theme() -> ThemeConfig {
    theme::load()
}

#[tauri::command]
pub fn save_theme(preset: String, custom_css: String) -> CmdResult<ThemeConfig> {
    let cfg = ThemeConfig { preset, custom_css };
    theme::save(&cfg).map_err(err)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::is_soundcloud_page;

    #[test]
    fn open_url_allows_only_soundcloud_pages() {
        assert!(is_soundcloud_page("https://soundcloud.com/artist/track"));
        assert!(is_soundcloud_page("https://m.soundcloud.com/x"));
        for bad in [
            "http://soundcloud.com/x",
            "https://soundcloud.com.evil.com/x",
            "https://evilsoundcloud.com/",
            "https://user:pw@soundcloud.com/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "",
        ] {
            assert!(!is_soundcloud_page(bad), "{bad}");
        }
    }
}

#[tauri::command]
pub async fn get_discover(state: State<'_, AppState>) -> CmdResult<Vec<crate::models::DiscoverySection>> {
    state.api.discover().await.map_err(err)
}

#[tauri::command]
pub async fn get_system_playlist_tracks(state: State<'_, AppState>, system_id: String) -> CmdResult<Vec<Track>> {
    state.api.system_playlist_tracks(&system_id).await.map_err(err)
}
