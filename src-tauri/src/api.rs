//! SoundCloud API client.
//!
//! Two backends are supported:
//! * **Official** — `api.soundcloud.com`, authorized with an OAuth access token
//!   obtained through the PKCE flow in [`crate::auth`].
//! * **V2** — `api-v2.soundcloud.com` (the private API used by the web app),
//!   authorized with a `client_id` (user supplied or scraped from the web
//!   app's JS bundles) and optionally a user's `oauth_token`.
//!
//! Security rules enforced here:
//! * credentials are only ever sent to the configured API origins — absolute
//!   URLs (`next_href`, transcoding URLs, which may come from the frontend)
//!   are checked against them;
//! * other fetches (waveforms, stream bodies) are limited to SoundCloud media
//!   hosts;
//! * error messages never contain tokens or full request URLs.

use crate::config::{self, AuthStore, OfficialSession};
use crate::models::{Page, Playlist, Track, User};
use anyhow::{anyhow, bail, Context, Result};
use parking_lot::RwLock;
use regex::Regex;
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

pub const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

/// Overall deadline for a JSON API request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Max time between two reads on any connection (catches stalled streams).
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Retries for 429 / 5xx / transient network errors.
const MAX_RETRIES: u32 = 2;
const MAX_RETRY_WAIT: Duration = Duration::from_secs(10);

/// Errors the UI may want to tell apart. Messages are safe to show: they
/// never include tokens or URLs.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("SoundCloud ограничил частоту запросов (HTTP 429). Попробуйте чуть позже.")]
    RateLimited,
    #[error("{0}")]
    Unauthorized(&'static str),
    #[error("SoundCloud ответил ошибкой HTTP {0}")]
    Status(u16),
    #[error("сетевая ошибка: {0}")]
    Network(String),
    #[error("некорректный ответ SoundCloud API")]
    BadResponse,
    #[error("запрос к постороннему адресу заблокирован")]
    ForeignUrl,
}

/// Base URLs of every remote endpoint (overridable in tests).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub official: Url,
    pub v2: Url,
    pub token: Url,
    /// soundcloud.com web app (client_id scraping).
    pub web: Url,
    /// Hosts we may fetch media from without credentials (suffix match).
    pub media_hosts: Vec<String>,
}

impl Default for Endpoints {
    fn default() -> Self {
        let u = |s: &str| Url::parse(s).expect("static endpoint URL is valid");
        Endpoints {
            official: u("https://api.soundcloud.com/"),
            v2: u("https://api-v2.soundcloud.com/"),
            token: u("https://secure.soundcloud.com/oauth/token"),
            web: u("https://soundcloud.com/"),
            media_hosts: vec!["sndcdn.com".into(), "soundcloud.com".into()],
        }
    }
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme() && a.host_str() == b.host_str() && a.port_or_known_default() == b.port_or_known_default()
}

/// `u` is the API at `base` (same origin and under its path).
pub fn within(base: &Url, u: &Url) -> bool {
    same_origin(base, u) && u.path().starts_with(base.path())
}

impl Endpoints {
    /// Resolve a relative API path, or validate an absolute URL against `base`.
    fn resolve(base: &Url, path: &str) -> Result<Url> {
        if path.starts_with("http://") || path.starts_with("https://") {
            let u = Url::parse(path).map_err(|_| ApiError::ForeignUrl)?;
            if !within(base, &u) {
                return Err(ApiError::ForeignUrl.into());
            }
            Ok(u)
        } else {
            Ok(base.join(path.trim_start_matches('/'))?)
        }
    }

    /// Credential-free media fetches (waveforms) are limited to these hosts.
    pub fn is_media_url(&self, u: &Url) -> bool {
        let Some(host) = u.host_str() else { return false };
        let host_ok = self.media_hosts.iter().any(|h| host == h || host.ends_with(&format!(".{h}")));
        let local = host == "127.0.0.1" || host == "localhost";
        host_ok && (u.scheme() == "https" || (local && u.scheme() == "http"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Official OAuth session.
    Official,
    /// api-v2 with a user oauth_token.
    V2,
    /// api-v2 with only a client_id (search + playback, no library).
    Anonymous,
}

/// Where the audio data for a track can be fetched from.
#[derive(Clone)]
pub struct StreamSource {
    pub url: String,
    /// `Authorization` header value, only set for official API URLs.
    pub auth: Option<String>,
    /// Container/codec hint for the decoder ("mp3" or "mp4").
    pub hint: &'static str,
    pub preview: bool,
}

impl fmt::Debug for StreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamSource")
            .field("auth", &self.auth.as_ref().map(|_| "<redacted>"))
            .field("hint", &self.hint)
            .field("preview", &self.preview)
            .finish_non_exhaustive()
    }
}

/// Strip the URL (it may carry client_id / signed CDN params) from errors.
pub fn net_err(e: reqwest::Error) -> anyhow::Error {
    let kind = if e.is_timeout() {
        "превышено время ожидания"
    } else if e.is_connect() {
        "не удалось подключиться"
    } else {
        "ошибка соединения"
    };
    ApiError::Network(format!("{kind} ({})", e.without_url())).into()
}

pub fn build_http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .gzip(true)
        .build()?)
}

fn retry_after(resp: &Response) -> Option<Duration> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(400 * 2u64.pow(attempt))
}

/// Send a request, retrying 429 / 502-504 (honouring `Retry-After`) and —
/// for idempotent requests — transient network failures.
pub async fn execute(req: RequestBuilder, idempotent: bool) -> Result<Response> {
    let mut attempt = 0;
    loop {
        let r = req.try_clone().ok_or_else(|| anyhow!("request cannot be retried"))?;
        let resp = match r.send().await {
            Ok(resp) => resp,
            Err(e) if idempotent && attempt < MAX_RETRIES && (e.is_timeout() || e.is_connect()) => {
                attempt += 1;
                tokio::time::sleep(backoff(attempt)).await;
                continue;
            }
            Err(e) => return Err(net_err(e)),
        };
        let status = resp.status();
        let transient = matches!(status.as_u16(), 429 | 502 | 503 | 504);
        if transient && attempt < MAX_RETRIES {
            let wait = retry_after(&resp).unwrap_or_else(|| backoff(attempt + 1)).min(MAX_RETRY_WAIT);
            log::warn!("HTTP {status}, retrying in {wait:?}");
            attempt += 1;
            tokio::time::sleep(wait).await;
            continue;
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(ApiError::RateLimited.into());
        }
        return Ok(resp);
    }
}

/// Empty body → `Null`; anything that isn't JSON is an error.
fn parse_json(text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(text).map_err(|_| ApiError::BadResponse.into())
}

async fn body_json(resp: Response) -> Result<Value> {
    let text = resp.text().await.map_err(net_err)?;
    parse_json(&text)
}

pub struct ApiClient {
    pub http: reqwest::Client,
    ep: Endpoints,
    dir: PathBuf,
    store: RwLock<AuthStore>,
    scrape_lock: tokio::sync::Mutex<()>,
    refresh_lock: tokio::sync::Mutex<()>,
    me: RwLock<Option<User>>,
    liked: RwLock<HashSet<u64>>,
    waveforms: RwLock<HashMap<String, Vec<f32>>>,
}

impl ApiClient {
    pub fn new() -> Result<Self> {
        Self::with_endpoints(Endpoints::default(), config::config_dir())
    }

    pub fn with_endpoints(ep: Endpoints, config_dir: PathBuf) -> Result<Self> {
        Ok(ApiClient {
            http: build_http()?,
            store: RwLock::new(config::load_from(&config_dir, "auth.json")),
            ep,
            dir: config_dir,
            scrape_lock: Default::default(),
            refresh_lock: Default::default(),
            me: RwLock::new(None),
            liked: RwLock::new(HashSet::new()),
            waveforms: RwLock::new(HashMap::new()),
        })
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.ep
    }

    pub fn config_dir(&self) -> &Path {
        &self.dir
    }

    // ------------------------------------------------------------------
    // Auth store helpers
    // ------------------------------------------------------------------

    pub fn store(&self) -> AuthStore {
        self.store.read().clone()
    }

    pub fn update_store(&self, f: impl FnOnce(&mut AuthStore)) -> Result<()> {
        let snapshot = {
            let mut s = self.store.write();
            f(&mut s);
            s.clone()
        };
        config::save_to(&self.dir, "auth.json", &snapshot).context("не удалось сохранить auth.json")
    }

    pub fn mode(&self) -> Mode {
        let s = self.store.read();
        if s.official.is_some() {
            Mode::Official
        } else if s.v2_oauth_token.is_some() {
            Mode::V2
        } else {
            Mode::Anonymous
        }
    }

    pub fn clear_session_cache(&self) {
        *self.me.write() = None;
        self.liked.write().clear();
    }

    pub fn is_liked(&self, id: u64) -> bool {
        self.liked.read().contains(&id)
    }

    // ------------------------------------------------------------------
    // client_id handling (api-v2)
    // ------------------------------------------------------------------

    /// The client_id used for api-v2: the user's own, or a scraped one.
    pub async fn client_id(&self) -> Result<String> {
        {
            let s = self.store.read();
            if let Some(id) = s.v2_client_id.clone().or_else(|| s.scraped_client_id.clone()) {
                return Ok(id);
            }
        }
        self.scrape_client_id(false).await
    }

    /// Fetch a fresh public client_id from soundcloud.com.
    pub async fn scrape_client_id(&self, force: bool) -> Result<String> {
        let _guard = self.scrape_lock.lock().await;
        if !force {
            if let Some(id) = self.store.read().scraped_client_id.clone() {
                return Ok(id);
            }
        }
        let id = self.scrape().await?;
        log::info!("scraped a new public client_id ({})", config::mask(&id));
        let id2 = id.clone();
        self.update_store(move |s| {
            s.scraped_client_id = Some(id2);
            s.scraped_at = Some(config::now_unix());
        })?;
        Ok(id)
    }

    async fn get_text(&self, url: Url) -> Result<String> {
        let resp = execute(self.http.get(url).timeout(REQUEST_TIMEOUT), true).await?;
        if !resp.status().is_success() {
            return Err(ApiError::Status(resp.status().as_u16()).into());
        }
        resp.text().await.map_err(net_err)
    }

    async fn scrape(&self) -> Result<String> {
        let html = self.get_text(self.ep.web.clone()).await?;
        // Newer pages embed it in the hydration data.
        let hydration = Regex::new(r#""hydratable"\s*:\s*"apiClient"\s*,\s*"data"\s*:\s*\{\s*"id"\s*:\s*"([A-Za-z0-9]{32})""#)?;
        if let Some(id) = hydration.captures(&html).and_then(|c| c.get(1)) {
            return Ok(id.as_str().to_owned());
        }
        let script_re = Regex::new(r#"<script[^>]+src="([^"]+/assets/[^"]+\.js)""#)?;
        let scripts: Vec<Url> = script_re
            .captures_iter(&html)
            .filter_map(|c| c.get(1))
            .filter_map(|m| self.ep.web.join(m.as_str()).ok())
            // Only SoundCloud's own bundles.
            .filter(|u| self.ep.is_media_url(u) || same_origin(u, &self.ep.web))
            .collect();
        if scripts.is_empty() {
            bail!("не найдены JS-бандлы на soundcloud.com");
        }
        let id_re = Regex::new(r#"client_id\s*[:=]\s*"([A-Za-z0-9]{32})""#)?;
        let id_re2 = Regex::new(r#"[?&]client_id=([A-Za-z0-9]{32})"#)?;
        // The id usually lives in one of the last bundles.
        for src in scripts.into_iter().rev() {
            let Ok(js) = self.get_text(src).await else { continue };
            if let Some(id) = id_re.captures(&js).or_else(|| id_re2.captures(&js)).and_then(|c| c.get(1)) {
                return Ok(id.as_str().to_owned());
            }
        }
        bail!("не удалось найти client_id в JS-бандлах SoundCloud")
    }

    // ------------------------------------------------------------------
    // Low level requests
    // ------------------------------------------------------------------

    /// Request against api-v2. Retries once with a freshly scraped client_id
    /// when SoundCloud has rotated the cached one (401/403).
    pub async fn v2_request(&self, method: Method, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let mut rescraped = false;
        loop {
            let mut url = Endpoints::resolve(&self.ep.v2, path)?;
            let client_id = self.client_id().await?;
            // Drop a stale client_id that next_href URLs may carry.
            let pairs: Vec<(String, String)> = url
                .query_pairs()
                .filter(|(k, _)| k != "client_id")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            {
                let mut qp = url.query_pairs_mut();
                qp.clear();
                for (k, v) in &pairs {
                    qp.append_pair(k, v);
                }
                for (k, v) in query {
                    qp.append_pair(k, v);
                }
                qp.append_pair("client_id", &client_id);
                qp.append_pair("app_locale", "en");
            }
            let mut req = self
                .http
                .request(method.clone(), url)
                .header("Accept", "application/json")
                .timeout(REQUEST_TIMEOUT);
            if let Some(tok) = self.store.read().v2_oauth_token.clone() {
                req = req.header("Authorization", format!("OAuth {tok}"));
            }
            let resp = execute(req, method != Method::POST).await?;
            let status = resp.status();
            if status.is_success() {
                return body_json(resp).await;
            }
            let auth_failure = status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN;
            let user_supplied_id = self.store.read().v2_client_id.is_some();
            if auth_failure && !rescraped && !user_supplied_id {
                log::warn!("api-v2 returned {status}, re-scraping client_id");
                self.scrape_client_id(true).await?;
                rescraped = true;
                continue;
            }
            if status == StatusCode::UNAUTHORIZED {
                return Err(ApiError::Unauthorized(
                    "SoundCloud отклонил авторизацию (401). Проверьте client_id / oauth_token в настройках.",
                )
                .into());
            }
            return Err(ApiError::Status(status.as_u16()).into());
        }
    }

    pub async fn v2_get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.v2_request(Method::GET, path, query).await
    }

    async fn official_token(&self) -> Result<String> {
        let sess = self.store.read().official.clone().ok_or_else(|| anyhow!("нет OAuth-сессии"))?;
        let expiring = sess.expires_at.map(|t| t - 60 < config::now_unix()).unwrap_or(false);
        if expiring && sess.refresh_token.is_some() {
            return self.refresh_official(false).await;
        }
        Ok(sess.access_token)
    }

    /// Exchange the refresh token for a new access token. `force` skips the
    /// "someone else already refreshed" shortcut (used after a 401).
    pub async fn refresh_official(&self, force: bool) -> Result<String> {
        let before = self.store.read().official.as_ref().map(|o| o.access_token.clone());
        let _guard = self.refresh_lock.lock().await;
        let s = self.store();
        let sess = s.official.clone().ok_or_else(|| anyhow!("нет OAuth-сессии"))?;
        // Another task may have refreshed while we waited for the lock.
        let refreshed_meanwhile = before.as_deref() != Some(sess.access_token.as_str());
        let still_valid = sess.expires_at.map(|t| t - 60 > config::now_unix()).unwrap_or(false);
        if refreshed_meanwhile || (!force && still_valid) {
            return Ok(sess.access_token);
        }
        let refresh = sess.refresh_token.ok_or(ApiError::Unauthorized("Сессия истекла — войдите заново"))?;
        let client_id = s.app_client_id.ok_or_else(|| anyhow!("нет client_id приложения"))?;
        let client_secret = s.app_client_secret.unwrap_or_default();
        let form = [
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh.as_str()),
        ];
        let mut new = self.request_token(&form).await?;
        if new.refresh_token.is_none() {
            new.refresh_token = Some(refresh.clone());
        }
        let tok = new.access_token.clone();
        self.update_store(|s| s.official = Some(new))?;
        log::info!("OAuth access token refreshed");
        Ok(tok)
    }

    pub async fn official_request(&self, method: Method, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let url = Endpoints::resolve(&self.ep.official, path)?;
        let mut refreshed = false;
        loop {
            let token = self.official_token().await?;
            let req = self
                .http
                .request(method.clone(), url.clone())
                .query(query)
                .header("Accept", "application/json; charset=utf-8")
                .header("Authorization", format!("OAuth {token}"))
                .timeout(REQUEST_TIMEOUT);
            let resp = execute(req, method != Method::POST).await?;
            let status = resp.status();
            if status == StatusCode::UNAUTHORIZED && !refreshed {
                refreshed = true;
                match self.refresh_official(true).await {
                    Ok(_) => continue,
                    Err(e) => {
                        log::warn!("token refresh failed: {e:#}");
                        return Err(ApiError::Unauthorized("Сессия истекла — войдите заново").into());
                    }
                }
            }
            if status == StatusCode::UNAUTHORIZED {
                return Err(ApiError::Unauthorized("Сессия истекла — войдите заново").into());
            }
            if !status.is_success() {
                return Err(ApiError::Status(status.as_u16()).into());
            }
            return body_json(resp).await;
        }
    }

    pub async fn official_get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.official_request(Method::GET, path, query).await
    }

    /// POST to the OAuth token endpoint (authorization_code or refresh_token).
    /// Never retried on network errors: an authorization code is single-use.
    pub async fn request_token(&self, form: &[(&str, &str)]) -> Result<OfficialSession> {
        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
            scope: Option<String>,
        }
        let req = self
            .http
            .post(self.ep.token.clone())
            .header("Accept", "application/json; charset=utf-8")
            .form(form)
            .timeout(REQUEST_TIMEOUT);
        let resp = execute(req, false).await?;
        let status = resp.status();
        let body = resp.text().await.map_err(net_err)?;
        if !status.is_success() {
            // Only surface the standard OAuth error fields, never the raw body.
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let field = |k: &str| v.get(k).and_then(Value::as_str).map(|s| s.chars().take(160).collect::<String>());
            let detail = match (field("error"), field("error_description")) {
                (Some(e), Some(d)) => format!(": {e} — {d}"),
                (Some(e), None) => format!(": {e}"),
                _ => String::new(),
            };
            bail!("ошибка обмена токена (HTTP {status}){detail}");
        }
        let t: TokenResponse = serde_json::from_str(&body).map_err(|_| ApiError::BadResponse)?;
        if t.access_token.is_empty() {
            return Err(ApiError::BadResponse.into());
        }
        Ok(OfficialSession {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            expires_at: t.expires_in.map(|e| config::now_unix() + e),
            scope: t.scope,
        })
    }

    // ------------------------------------------------------------------
    // Collections
    // ------------------------------------------------------------------

    /// Parse `{collection, next_href}` or a bare array into tracks, resolving
    /// v2 "stub" tracks that carry only an id. Malformed items are skipped.
    async fn to_page(&self, v: Value) -> Result<Page> {
        let next_href = v.get("next_href").and_then(Value::as_str).map(str::to_owned);
        let items: Vec<Value> = match v {
            Value::Array(a) => a,
            Value::Object(mut o) => match o.remove("collection") {
                Some(Value::Array(a)) => a,
                Some(_) => return Err(ApiError::BadResponse.into()),
                None => Vec::new(),
            },
            Value::Null => Vec::new(),
            _ => return Err(ApiError::BadResponse.into()),
        };
        let tracks = self.tracks_from_items(&items).await?;
        Ok(Page { tracks, next_href })
    }

    async fn tracks_from_items(&self, items: &[Value]) -> Result<Vec<Track>> {
        let mut stub_ids = Vec::new();
        for it in items {
            let t = it.get("track").filter(|t| t.is_object()).unwrap_or(it);
            if Track::is_stub(t) && t.get("kind").and_then(Value::as_str).unwrap_or("track") == "track" {
                if let Some(id) = t.get("id").and_then(Value::as_u64) {
                    stub_ids.push(id);
                }
            }
        }
        let resolved = if stub_ids.is_empty() || self.mode() == Mode::Official {
            HashMap::new()
        } else {
            self.v2_tracks_by_ids(&stub_ids).await.unwrap_or_default()
        };
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for it in items {
            let t = it.get("track").filter(|t| t.is_object()).unwrap_or(it);
            let track = if Track::is_stub(t) {
                t.get("id").and_then(Value::as_u64).and_then(|id| resolved.get(&id).cloned())
            } else {
                Track::from_value(it)
            };
            if let Some(track) = track {
                if seen.insert(track.id) {
                    out.push(track);
                }
            }
        }
        Ok(out)
    }

    async fn v2_tracks_by_ids(&self, ids: &[u64]) -> Result<HashMap<u64, Track>> {
        let mut map = HashMap::new();
        for chunk in ids.chunks(50) {
            let joined = chunk.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
            let v = self.v2_get("/tracks", &[("ids", joined)]).await?;
            if let Some(arr) = v.as_array() {
                for t in arr.iter().filter_map(Track::from_value) {
                    map.insert(t.id, t);
                }
            }
        }
        Ok(map)
    }

    pub async fn me(&self) -> Result<Option<User>> {
        if let Some(u) = self.me.read().clone() {
            return Ok(Some(u));
        }
        let v = match self.mode() {
            Mode::Official => self.official_get("/me", &[]).await?,
            Mode::V2 => self.v2_get("/me", &[]).await?,
            Mode::Anonymous => return Ok(None),
        };
        let user = User::from_value(&v);
        *self.me.write() = user.clone();
        Ok(user)
    }

    async fn my_id(&self) -> Result<u64> {
        self.me()
            .await?
            .map(|u| u.id)
            .ok_or_else(|| anyhow!("Для медиатеки нужен вход в аккаунт (OAuth или oauth_token)"))
    }

    /// Home screen: subscription feed when logged in, charts otherwise.
    pub async fn feed(&self) -> Result<Page> {
        match self.mode() {
            Mode::Official => {
                let q = [("limit", "40".to_string()), ("linked_partitioning", "true".to_string())];
                match self.official_get("/me/feed/tracks", &q).await {
                    Ok(v) => self.to_page(v).await,
                    Err(e) => {
                        log::warn!("feed failed: {e:#}, falling back to likes");
                        self.likes().await
                    }
                }
            }
            Mode::V2 => {
                let v = self
                    .v2_get("/stream", &[("limit", "40".into()), ("linked_partitioning", "1".into())])
                    .await?;
                self.to_page(v).await
            }
            Mode::Anonymous => self.charts().await,
        }
    }

    async fn charts(&self) -> Result<Page> {
        let q = [
            ("kind", "trending".to_string()),
            ("genre", "soundcloud:genres:all-music".to_string()),
            ("limit", "40".to_string()),
            ("linked_partitioning", "1".to_string()),
        ];
        if let Ok(v) = self.v2_get("/charts", &q).await {
            let page = self.to_page(v).await?;
            if !page.tracks.is_empty() {
                return Ok(page);
            }
        }
        // Charts endpoint is not always available: fall back to a search.
        self.search("new music this week").await
    }

    pub async fn search(&self, query: &str) -> Result<Page> {
        let v = match self.mode() {
            Mode::Official => {
                let q = [
                    ("q", query.to_string()),
                    ("limit", "30".to_string()),
                    ("linked_partitioning", "true".to_string()),
                    ("access", "playable,preview".to_string()),
                ];
                self.official_get("/tracks", &q).await?
            }
            _ => {
                let q = [
                    ("q", query.to_string()),
                    ("limit", "30".to_string()),
                    ("linked_partitioning", "1".to_string()),
                ];
                self.v2_get("/search/tracks", &q).await?
            }
        };
        self.to_page(v).await
    }

    pub async fn likes(&self) -> Result<Page> {
        let page = match self.mode() {
            Mode::Official => {
                let q = [
                    ("limit", "50".to_string()),
                    ("linked_partitioning", "true".to_string()),
                    ("access", "playable,preview".to_string()),
                ];
                let v = self.official_get("/me/likes/tracks", &q).await?;
                self.to_page(v).await?
            }
            _ => {
                let id = self.my_id().await?;
                let q = [("limit", "50".to_string()), ("linked_partitioning", "1".to_string())];
                let v = self.v2_get(&format!("/users/{id}/track_likes"), &q).await?;
                self.to_page(v).await?
            }
        };
        self.liked.write().extend(page.tracks.iter().map(|t| t.id));
        Ok(page)
    }

    /// Follow a `next_href` returned by a previous page. The URL comes from
    /// the frontend, so it must belong to one of the API origins.
    pub async fn next_page(&self, next_href: &str) -> Result<Page> {
        let u = Url::parse(next_href).map_err(|_| ApiError::ForeignUrl)?;
        let v = if within(&self.ep.official, &u) {
            self.official_get(next_href, &[]).await?
        } else if within(&self.ep.v2, &u) {
            self.v2_get(next_href, &[]).await?
        } else {
            return Err(ApiError::ForeignUrl.into());
        };
        self.to_page(v).await
    }

    /// Ids of all liked tracks (to render the heart button state).
    pub async fn liked_ids(&self) -> Result<Vec<u64>> {
        let ids: Vec<u64> = match self.mode() {
            Mode::Anonymous => Vec::new(),
            Mode::V2 => {
                let v = self.v2_get("/me/track_likes/ids", &[("limit", "5000".into())]).await?;
                v.get("collection")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_u64).collect())
                    .unwrap_or_default()
            }
            Mode::Official => {
                let mut ids = Vec::new();
                let q = [("limit", "200".to_string()), ("linked_partitioning", "true".to_string())];
                let mut v = self.official_get("/me/likes/tracks", &q).await?;
                for _ in 0..5 {
                    if let Some(arr) = v.get("collection").and_then(Value::as_array) {
                        ids.extend(arr.iter().filter_map(|t| t.get("id").and_then(Value::as_u64)));
                    }
                    match v.get("next_href").and_then(Value::as_str).map(str::to_owned) {
                        Some(next) => v = self.official_get(&next, &[]).await?,
                        None => break,
                    }
                }
                ids
            }
        };
        let mut liked = self.liked.write();
        liked.clear();
        liked.extend(ids.iter().copied());
        Ok(ids)
    }

    pub async fn set_like(&self, track_id: u64, like: bool) -> Result<()> {
        match self.mode() {
            Mode::Anonymous => bail!("Лайки доступны только после входа в аккаунт"),
            Mode::Official => {
                let method = if like { Method::POST } else { Method::DELETE };
                self.official_request(method, &format!("/likes/tracks/{track_id}"), &[]).await?;
            }
            Mode::V2 => {
                let uid = self.my_id().await?;
                let method = if like { Method::PUT } else { Method::DELETE };
                self.v2_request(method, &format!("/users/{uid}/track_likes/{track_id}"), &[]).await?;
            }
        }
        let mut liked = self.liked.write();
        if like {
            liked.insert(track_id);
        } else {
            liked.remove(&track_id);
        }
        Ok(())
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut values = Vec::new();
        match self.mode() {
            Mode::Official => {
                let q = [
                    ("limit", "50".to_string()),
                    ("linked_partitioning", "true".to_string()),
                    ("show_tracks", "false".to_string()),
                ];
                values.push(self.official_get("/me/playlists", &q).await?);
                if let Ok(v) = self.official_get("/me/likes/playlists", &q).await {
                    values.push(v);
                }
            }
            _ => {
                let id = self.my_id().await?;
                let q = [("limit", "50".to_string()), ("linked_partitioning", "1".to_string())];
                values.push(self.v2_get(&format!("/users/{id}/playlists_without_albums"), &q).await?);
                if let Ok(v) = self.v2_get(&format!("/users/{id}/playlist_likes"), &q).await {
                    values.push(v);
                }
            }
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for v in values {
            let items = match &v {
                Value::Array(a) => a.clone(),
                _ => v.get("collection").and_then(Value::as_array).cloned().unwrap_or_default(),
            };
            for p in items.iter().filter_map(Playlist::from_value) {
                if seen.insert(p.id) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    pub async fn playlist_tracks(&self, playlist_id: u64) -> Result<Vec<Track>> {
        match self.mode() {
            Mode::Official => {
                let q = [
                    ("limit", "200".to_string()),
                    ("linked_partitioning", "true".to_string()),
                    ("access", "playable,preview".to_string()),
                ];
                let first = self.official_get(&format!("/playlists/{playlist_id}/tracks"), &q).await?;
                let mut page = self.to_page(first).await?;
                let mut tracks = std::mem::take(&mut page.tracks);
                let mut next = page.next_href;
                for _ in 0..10 {
                    let Some(href) = next.take() else { break };
                    let p = self.next_page(&href).await?;
                    tracks.extend(p.tracks);
                    next = p.next_href;
                }
                Ok(tracks)
            }
            _ => {
                let v = self
                    .v2_get(&format!("/playlists/{playlist_id}"), &[("representation", "full".into())])
                    .await?;
                let items = v.get("tracks").and_then(Value::as_array).cloned().unwrap_or_default();
                self.tracks_from_items(&items).await
            }
        }
    }

    // ------------------------------------------------------------------
    // Streaming
    // ------------------------------------------------------------------

    pub async fn resolve_stream(&self, track: &Track) -> Result<StreamSource> {
        match self.mode() {
            Mode::Official => self.resolve_official(track).await,
            _ => self.resolve_v2(track).await,
        }
    }

    async fn resolve_official(&self, track: &Track) -> Result<StreamSource> {
        let v = self.official_get(&format!("/tracks/{}/streams", track.id), &[]).await?;
        let token = self.official_token().await?;
        let candidates = [
            ("http_mp3_128_url", "mp3", false),
            ("hls_mp3_128_url", "mp3", false),
            ("hls_aac_160_url", "mp4", false),
            ("preview_mp3_128_url", "mp3", true),
        ];
        for (key, hint, preview) in candidates {
            let Some(url) = v.get(key).and_then(Value::as_str) else { continue };
            let Ok(parsed) = Url::parse(url) else { continue };
            // Only the API itself gets the token; CDN URLs are pre-signed.
            let auth = within(&self.ep.official, &parsed).then(|| format!("OAuth {token}"));
            if auth.is_none() && !self.ep.is_media_url(&parsed) {
                continue;
            }
            return Ok(StreamSource { url: url.to_owned(), auth, hint, preview });
        }
        // Tracks obtained via official API sometimes have no streams for the
        // app; try the web API as a last resort.
        self.resolve_v2(track).await
    }

    async fn resolve_v2(&self, track: &Track) -> Result<StreamSource> {
        let mut track = track.clone();
        let has_transcodings = track
            .media
            .as_ref()
            .and_then(|m| m.get("transcodings"))
            .and_then(Value::as_array)
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if !has_transcodings {
            let v = self.v2_get(&format!("/tracks/{}", track.id), &[]).await?;
            track = Track::from_value(&v).ok_or_else(|| anyhow!("трек недоступен"))?;
        }
        let transcodings = track
            .media
            .as_ref()
            .and_then(|m| m.get("transcodings"))
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| anyhow!("у трека нет доступных потоков"))?;

        let (tc, hint) = pick_transcoding(&transcodings)
            .ok_or_else(|| anyhow!("нет поддерживаемого (незашифрованного) потока для этого трека"))?;
        let preview = tc.get("snipped").and_then(Value::as_bool).unwrap_or(false);
        let tc_url = tc.get("url").and_then(Value::as_str).ok_or(ApiError::BadResponse)?;
        let mut q = Vec::new();
        if let Some(auth) = track.track_authorization.clone() {
            q.push(("track_authorization", auth));
        }
        // `tc_url` may originate from the frontend: v2_get refuses foreign hosts.
        let v = self.v2_get(tc_url, &q).await?;
        let url = v
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("SoundCloud не вернул ссылку на поток"))?;
        Ok(StreamSource { url: url.to_owned(), auth: None, hint, preview })
    }

    // ------------------------------------------------------------------
    // Waveforms
    // ------------------------------------------------------------------

    /// Download the waveform JSON and reduce it to `bars` values in 0..=1.
    pub async fn waveform(&self, waveform_url: &str, bars: usize) -> Result<Vec<f32>> {
        let key = format!("{waveform_url}#{bars}");
        if let Some(w) = self.waveforms.read().get(&key) {
            return Ok(w.clone());
        }
        let json_url = match waveform_url.strip_suffix(".png") {
            Some(base) => format!("{base}.json"),
            None => waveform_url.to_owned(),
        };
        let url = Url::parse(&json_url).map_err(|_| ApiError::ForeignUrl)?;
        if !self.ep.is_media_url(&url) {
            return Err(ApiError::ForeignUrl.into());
        }
        let v = parse_json(&self.get_text(url).await?)?;
        let height = v.get("height").and_then(Value::as_f64).filter(|h| *h > 0.0).unwrap_or(140.0);
        let samples: Vec<f64> = v
            .get("samples")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).filter(|x| x.is_finite()).collect())
            .unwrap_or_default();
        if samples.is_empty() {
            bail!("пустая вейвформа");
        }
        let out = downsample(&samples, bars.clamp(16, 1000), height);
        let mut cache = self.waveforms.write();
        if cache.len() > 2000 {
            cache.clear();
        }
        cache.insert(key, out.clone());
        Ok(out)
    }
}

/// Choose the best transcoding we can decode: progressive MP3 first, then HLS
/// MP3, then HLS AAC. Encrypted (DRM) streams are skipped.
pub fn pick_transcoding(list: &[Value]) -> Option<(Value, &'static str)> {
    let mut best: Option<(u32, Value, &'static str)> = None;
    for tc in list {
        let format = tc.get("format").cloned().unwrap_or(Value::Null);
        let protocol = format.get("protocol").and_then(Value::as_str).unwrap_or("");
        let mime = format.get("mime_type").and_then(Value::as_str).unwrap_or("");
        let snipped = tc.get("snipped").and_then(Value::as_bool).unwrap_or(false);
        let preset = tc.get("preset").and_then(Value::as_str).unwrap_or("");
        let base_mime = mime.split(';').next().unwrap_or("").trim();
        let is_aac = base_mime == "audio/mp4" && mime.contains("mp4a");
        let (score, hint) = match (protocol, base_mime) {
            ("progressive", "audio/mpeg") => (0, "mp3"),
            ("hls", "audio/mpeg") => (1, "mp3"),
            ("hls" | "progressive", _) if is_aac && preset.contains("160") => (2, "mp4"),
            ("hls" | "progressive", _) if is_aac => (3, "mp4"),
            // Adaptive master playlist (variants are AAC).
            ("hls", "audio/mpegurl") => (4, "mp4"),
            _ => continue, // encrypted / opus / unknown
        };
        let score = score + if snipped { 10 } else { 0 };
        if best.as_ref().map(|(s, _, _)| score < *s).unwrap_or(true) {
            best = Some((score, tc.clone(), hint));
        }
    }
    best.map(|(_, v, h)| (v, h))
}

fn downsample(samples: &[f64], bars: usize, height: f64) -> Vec<f32> {
    let n = samples.len();
    let per = n as f64 / bars as f64;
    (0..bars)
        .map(|i| {
            let a = ((i as f64 * per) as usize).min(n - 1);
            let b = (((i + 1) as f64 * per) as usize).clamp(a + 1, n);
            let peak = samples[a..b].iter().cloned().fold(0.0, f64::max);
            (peak / height).clamp(0.02, 1.0) as f32
        })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Endpoints pointing at a mock server (`/official/`, `/v2/`, `/web/`, `/oauth/token`).
    pub fn mock_endpoints(base: &str) -> Endpoints {
        let u = |p: &str| Url::parse(&format!("{base}{p}")).unwrap();
        Endpoints {
            official: u("/official/"),
            v2: u("/v2/"),
            token: u("/oauth/token"),
            web: u("/web/"),
            media_hosts: vec!["sndcdn.com".into(), "127.0.0.1".into()],
        }
    }

    pub fn client(server: &MockServer, dir: &Path, store: AuthStore) -> ApiClient {
        config::save_to(dir, "auth.json", &store).unwrap();
        ApiClient::with_endpoints(mock_endpoints(&server.uri()), dir.to_path_buf()).unwrap()
    }

    fn track_json(id: u64) -> Value {
        json!({"kind":"track","id":id,"title":format!("t{id}"),"duration":1000,"user":{"id":1,"username":"u"}})
    }

    fn anon(client_id: &str) -> AuthStore {
        AuthStore { scraped_client_id: Some(client_id.into()), ..Default::default() }
    }

    #[test]
    fn transcoding_choice_prefers_mp3_then_aac_and_skips_drm() {
        let list = vec![
            json!({"url":"a","format":{"protocol":"ctr-encrypted-hls","mime_type":"audio/mp4; codecs=\"mp4a.40.2\""}}),
            json!({"url":"b","preset":"aac_96k","format":{"protocol":"hls","mime_type":"audio/mp4; codecs=\"mp4a.40.2\""}}),
            json!({"url":"c","preset":"aac_160k","format":{"protocol":"hls","mime_type":"audio/mp4; codecs=\"mp4a.40.2\""}}),
            json!({"url":"d","format":{"protocol":"hls","mime_type":"audio/mpegurl"}}),
        ];
        let (tc, hint) = pick_transcoding(&list).unwrap();
        assert_eq!((tc["url"].as_str(), hint), (Some("c"), "mp4"));
        let mut with_mp3 = list.clone();
        with_mp3.push(json!({"url":"e","format":{"protocol":"progressive","mime_type":"audio/mpeg"}}));
        assert_eq!(pick_transcoding(&with_mp3).unwrap().0["url"], "e");
        assert!(pick_transcoding(&list[..1]).is_none(), "DRM-only track must be rejected");
        assert!(pick_transcoding(&[json!({}), json!(null), json!(42)]).is_none());
    }

    #[test]
    fn downsample_handles_short_and_odd_inputs() {
        assert_eq!(downsample(&[70.0], 16, 140.0).len(), 16);
        assert!(downsample(&[0.0, 140.0, 280.0], 100, 140.0).iter().all(|v| (0.02..=1.0).contains(v)));
    }

    #[test]
    fn origin_checks() {
        let ep = Endpoints::default();
        let ok = Url::parse("https://api.soundcloud.com/me/likes?x=1").unwrap();
        assert!(within(&ep.official, &ok));
        for bad in [
            "https://api.soundcloud.com.evil.com/me",
            "http://api.soundcloud.com/me",
            "https://api.soundcloud.com:8443/me",
            "https://evil.com/?https://api.soundcloud.com/",
        ] {
            assert!(!within(&ep.official, &Url::parse(bad).unwrap()), "{bad}");
        }
        assert!(ep.is_media_url(&Url::parse("https://wave.sndcdn.com/x_m.json").unwrap()));
        assert!(!ep.is_media_url(&Url::parse("https://evilsndcdn.com/x").unwrap()));
        assert!(!ep.is_media_url(&Url::parse("http://wave.sndcdn.com/x").unwrap()));
        assert!(!ep.is_media_url(&Url::parse("file:///etc/passwd").unwrap()));
    }

    #[tokio::test]
    async fn search_parses_valid_items_and_skips_broken_ones() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/search/tracks"))
            .and(query_param("q", "x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "collection": [
                    track_json(1),
                    {"kind":"track","id":2},                 // stub: resolved via /tracks?ids=
                    {"kind":"track","title":"no id"},        // invalid: skipped
                    {"kind":"playlist","id":3,"title":"pl"}, // not a track
                    "garbage", null, 7,
                    track_json(1),                           // duplicate
                ],
                "next_href": "https://api-v2.soundcloud.com/search/tracks?offset=30"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v2/tracks"))
            .and(query_param("ids", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([track_json(2)])))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = client(&server, dir.path(), anon("CID"));
        let page = api.search("x").await.unwrap();
        let ids: Vec<u64> = page.tracks.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert!(page.next_href.is_some());
    }

    #[tokio::test]
    async fn invalid_json_is_an_error_not_an_empty_page() {
        let server = MockServer::start().await;
        Mock::given(path("/v2/search/tracks"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>oops"))
            .mount(&server)
            .await;
        Mock::given(path("/v2/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"collection": "nope"})))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = client(&server, dir.path(), AuthStore { v2_oauth_token: Some("t".into()), ..anon("CID") });
        let e = api.search("x").await.unwrap_err();
        assert!(matches!(e.downcast_ref::<ApiError>(), Some(ApiError::BadResponse)), "{e:#}");
        let e = api.feed().await.unwrap_err();
        assert!(matches!(e.downcast_ref::<ApiError>(), Some(ApiError::BadResponse)), "{e:#}");
    }

    #[tokio::test]
    async fn expired_client_id_is_rescraped_once() {
        let server = MockServer::start().await;
        let new_id = "N".repeat(32);
        Mock::given(path("/v2/search/tracks"))
            .and(query_param("client_id", "OLD"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path("/v2/search/tracks"))
            .and(query_param("client_id", new_id.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"collection":[track_json(5)]})))
            .expect(1)
            .mount(&server)
            .await;
        // The web page only links a JS bundle; the id is inside it.
        let html = format!(r#"<html><script crossorigin src="{}/web/assets/app-1.js"></script></html>"#, server.uri());
        Mock::given(path("/web/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html))
            .mount(&server)
            .await;
        Mock::given(path("/web/assets/app-1.js"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(r#"x={{client_id:"{new_id}",y:1}}"#)))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = client(&server, dir.path(), anon("OLD"));
        let page = api.search("q").await.unwrap();
        assert_eq!(page.tracks[0].id, 5);
        assert_eq!(api.store().scraped_client_id.as_deref(), Some(new_id.as_str()));
    }

    #[tokio::test]
    async fn rate_limit_is_retried_then_reported() {
        let server = MockServer::start().await;
        Mock::given(path("/v2/search/tracks"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(path("/v2/search/tracks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"collection":[track_json(9)]})))
            .with_priority(2)
            .mount(&server)
            .await;
        Mock::given(path("/v2/charts"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .expect(1 + MAX_RETRIES as u64)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = client(&server, dir.path(), anon("CID"));
        assert_eq!(api.search("q").await.unwrap().tracks[0].id, 9);
        let e = api.v2_get("/charts", &[]).await.unwrap_err();
        assert!(matches!(e.downcast_ref::<ApiError>(), Some(ApiError::RateLimited)));
    }

    #[tokio::test]
    async fn official_401_refreshes_token_and_retries() {
        let server = MockServer::start().await;
        Mock::given(path("/official/me"))
            .and(header("Authorization", "OAuth OLD-AT"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(path("/official/me"))
            .and(header("Authorization", "OAuth NEW-AT"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":42,"username":"me"})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=OLD-RT"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"NEW-AT","expires_in":3600})))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let store = AuthStore {
            app_client_id: Some("app".into()),
            app_client_secret: Some("sec".into()),
            official: Some(OfficialSession {
                access_token: "OLD-AT".into(),
                refresh_token: Some("OLD-RT".into()),
                expires_at: Some(config::now_unix() + 3600),
                scope: None,
            }),
            ..Default::default()
        };
        let api = client(&server, dir.path(), store);
        let me = api.me().await.unwrap().unwrap();
        assert_eq!(me.id, 42);
        let saved: AuthStore = config::load_from(dir.path(), "auth.json");
        let sess = saved.official.unwrap();
        assert_eq!(sess.access_token, "NEW-AT");
        assert_eq!(sess.refresh_token.as_deref(), Some("OLD-RT"), "refresh token must be kept");
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("auth.json")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[tokio::test]
    async fn token_errors_do_not_echo_the_response_body() {
        let server = MockServer::start().await;
        Mock::given(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error":"invalid_grant","error_description":"bad code","access_token":"LEAKED-SECRET"
            })))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = client(&server, dir.path(), AuthStore::default());
        let msg = format!("{:#}", api.request_token(&[("code", "SECRET-CODE")]).await.unwrap_err());
        assert!(msg.contains("invalid_grant"), "{msg}");
        assert!(!msg.contains("LEAKED-SECRET") && !msg.contains("SECRET-CODE"), "{msg}");
    }

    #[tokio::test]
    async fn credentials_are_never_sent_to_foreign_hosts() {
        let server = MockServer::start().await;
        let foreign = MockServer::start().await; // different port = different origin
        Mock::given(wiremock::matchers::any())
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"collection":[]})))
            .expect(0)
            .mount(&foreign)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = client(&server, dir.path(), AuthStore { v2_oauth_token: Some("2-TOKEN".into()), ..anon("CID") });

        let lookalike = format!("{}.evil/v2/x", server.uri());
        for href in [format!("{}/v2/stream", foreign.uri()), lookalike, "javascript:alert(1)".into()] {
            let e = api.next_page(&href).await.unwrap_err();
            assert!(matches!(e.downcast_ref::<ApiError>(), Some(ApiError::ForeignUrl)), "{href}: {e:#}");
        }
        // A forged track whose transcoding URL points elsewhere.
        let mut t = Track::from_value(&track_json(1)).unwrap();
        t.media = Some(json!({"transcodings":[{"url": format!("{}/steal", foreign.uri()),
            "format":{"protocol":"progressive","mime_type":"audio/mpeg"}}]}));
        let e = api.resolve_stream(&t).await.unwrap_err();
        assert!(matches!(e.downcast_ref::<ApiError>(), Some(ApiError::ForeignUrl)), "{e:#}");
        // Waveforms only from media hosts.
        assert!(api.waveform("https://evil.example/wave.json", 50).await.is_err());
        assert!(api.waveform("file:///etc/passwd", 50).await.is_err());
    }

    #[tokio::test]
    async fn waveform_is_parsed_and_validated() {
        let server = MockServer::start().await;
        Mock::given(path("/w_m.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"width":4,"height":100,"samples":[10,50,100,0]})))
            .mount(&server)
            .await;
        Mock::given(path("/bad_m.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"samples":"nope"})))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = client(&server, dir.path(), anon("CID"));
        let w = api.waveform(&format!("{}/w_m.png", server.uri()), 16).await.unwrap();
        assert_eq!(w.len(), 16);
        assert!(w.iter().all(|v| (0.02..=1.0).contains(v)));
        assert!(api.waveform(&format!("{}/bad_m.json", server.uri()), 16).await.is_err());
    }
}
