//! Persistent storage in `~/.config/sc-rust-client/`.
//!
//! * `auth.json`     — OAuth session, user-supplied tokens, scraped client_id
//! * `settings.json` — volume, output device, repeat mode
//! * `theme.json`    — UI theme
//!
//! The directory is created `0700` and every file `0600` *at creation time*
//! (never world-readable, not even briefly), then atomically renamed.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sc-rust-client")
}

const REDACTED: &str = "<redacted>";

fn redact(v: &Option<String>) -> Option<&'static str> {
    v.as_ref().map(|_| REDACTED)
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct OfficialSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix timestamp (seconds) when the access token expires.
    pub expires_at: Option<i64>,
    pub scope: Option<String>,
}

// Manual Debug impls: tokens must never end up in logs or panic messages.
impl fmt::Debug for OfficialSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OfficialSession")
            .field("access_token", &REDACTED)
            .field("refresh_token", &redact(&self.refresh_token))
            .field("expires_at", &self.expires_at)
            .field("scope", &self.scope)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct AuthStore {
    /// Credentials of a registered SoundCloud app (needed for official OAuth).
    pub app_client_id: Option<String>,
    pub app_client_secret: Option<String>,
    /// Session obtained through the OAuth 2.1 + PKCE flow.
    pub official: Option<OfficialSession>,
    /// Fallback: client_id / oauth_token copied from the web version.
    pub v2_client_id: Option<String>,
    pub v2_oauth_token: Option<String>,
    /// client_id scraped automatically from soundcloud.com JS bundles.
    pub scraped_client_id: Option<String>,
    pub scraped_at: Option<i64>,
    /// User finished the login screen (possibly choosing anonymous mode).
    #[serde(default)]
    pub onboarded: bool,
}

impl fmt::Debug for AuthStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthStore")
            .field("app_client_id", &self.app_client_id)
            .field("app_client_secret", &redact(&self.app_client_secret))
            .field("official", &self.official)
            .field("v2_client_id", &self.v2_client_id)
            .field("v2_oauth_token", &redact(&self.v2_oauth_token))
            .field("scraped_client_id", &self.scraped_client_id)
            .field("onboarded", &self.onboarded)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub volume: f32,
    pub output_device: Option<String>,
    #[serde(default)]
    pub repeat: RepeatMode,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings { volume: 0.8, output_device: None, repeat: RepeatMode::Off, close_to_tray: true }
    }
}

/// Read `dir/name`, falling back to `T::default()` if missing or invalid.
pub fn load_from<T: DeserializeOwned + Default>(dir: &Path, name: &str) -> T {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Atomically write `dir/name` as JSON with owner-only permissions.
pub fn save_to<T: Serialize>(dir: &Path, name: &str, value: &T) -> std::io::Result<()> {
    create_private_dir(dir)?;
    let path = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    let _ = std::fs::remove_file(&tmp);
    let bytes = serde_json::to_vec_pretty(value)?;
    {
        let mut f = private_file(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &path)
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    // Tighten a directory created by an older version.
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// Create a new file that is `0600` from the very first byte.
#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)
}

/// Tighten an existing config directory (created by an older version or by
/// hand): directory `0700`, regular files `0600`. Symlinks are left alone.
#[cfg(unix)]
pub fn secure_existing(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if !meta.is_dir() {
        return Ok(());
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_file() && entry.metadata()?.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn secure_existing(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn load<T: DeserializeOwned + Default>(name: &str) -> T {
    load_from(&config_dir(), name)
}

pub fn save<T: Serialize>(name: &str, value: &T) -> std::io::Result<()> {
    save_to(&config_dir(), name, value)
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Show only the edges of a secret-ish value, e.g. `QhVE…x9Zk`. Never panics.
pub fn mask(v: &str) -> String {
    let chars: Vec<char> = v.chars().collect();
    if chars.len() <= 8 {
        return "•".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn saved_files_are_owner_only() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("nested").join("sc-rust-client");
        let store = AuthStore { v2_oauth_token: Some("2-secret".into()), ..Default::default() };
        save_to(&dir, "auth.json", &store).unwrap();
        assert_eq!(mode(&dir.join("auth.json")), 0o600);
        assert_eq!(mode(&dir), 0o700);
        // Overwrite keeps the permissions and leaves no temp file behind.
        save_to(&dir, "auth.json", &store).unwrap();
        assert_eq!(mode(&dir.join("auth.json")), 0o600);
        assert!(!dir.join(".auth.json.tmp").exists());
        let back: AuthStore = load_from(&dir, "auth.json");
        assert_eq!(back.v2_oauth_token.as_deref(), Some("2-secret"));
    }

    #[test]
    fn existing_loose_directory_is_tightened() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("cfg");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        save_to(&dir, "x.json", &1).unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn secure_existing_tightens_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("cfg");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let f = dir.join("auth.json");
        std::fs::write(&f, "{}").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        let outside = tmp.path().join("outside.txt");
        std::fs::write(&outside, "x").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();

        secure_existing(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&f), 0o600);
        assert_eq!(mode(&outside), 0o644, "symlink targets must not be touched");
        secure_existing(&tmp.path().join("missing")).unwrap();
    }

    #[test]
    fn corrupt_file_falls_back_to_default() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("settings.json"), "{not json").unwrap();
        let s: Settings = load_from(tmp.path(), "settings.json");
        assert_eq!(s.volume, Settings::default().volume);
    }

    #[test]
    fn debug_output_redacts_secrets() {
        let store = AuthStore {
            app_client_secret: Some("APP-SECRET".into()),
            v2_oauth_token: Some("2-OAUTH".into()),
            official: Some(OfficialSession {
                access_token: "ACCESS-TOK".into(),
                refresh_token: Some("REFRESH-TOK".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let dbg = format!("{store:?} {:#?}", store.official);
        for secret in ["APP-SECRET", "2-OAUTH", "ACCESS-TOK", "REFRESH-TOK"] {
            assert!(!dbg.contains(secret), "{secret} leaked in Debug output");
        }
        assert!(dbg.contains(REDACTED));
    }

    #[test]
    fn mask_never_panics() {
        assert_eq!(mask(""), "");
        assert_eq!(mask("abc"), "•••");
        assert_eq!(mask("QhVEWrABCDEFGHIJx9Zk"), "QhVE…x9Zk");
        assert_eq!(mask("ёжикёжикёжик"), "ёжик…ёжик");
    }
}
