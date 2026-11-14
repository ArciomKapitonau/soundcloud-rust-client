//! OAuth 2.1 Authorization Code flow with PKCE.
//!
//! 1. Bind a one-shot HTTP server on the loopback interface only
//!    (`127.0.0.1` / `::1`, port 3000) — never `0.0.0.0`.
//! 2. Open `https://secure.soundcloud.com/authorize` in the default browser
//!    (the user can sign in with Google, Apple, e-mail…).
//! 3. Accept requests until one carries the expected CSRF `state`; anything
//!    else is answered with 400/404 and ignored, so a hostile local page
//!    cannot abort the login.
//! 4. Exchange the code for tokens, show a "you can return to the app" page,
//!    persist the session (`auth.json`, mode 0600).
//!
//! The server lives only inside [`login_with`]: it is dropped — and the port
//! released — as soon as a code arrives, on error, or after
//! [`LOGIN_TIMEOUT`].

use crate::api::ApiClient;
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::distributions::Alphanumeric;
use rand::rngs::OsRng;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const AUTHORIZE_URL: &str = "https://secure.soundcloud.com/authorize";
pub const CALLBACK_PORT: u16 = 3000;
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);
/// Per-connection read deadline (slow or idle local clients).
const READ_DEADLINE: Duration = Duration::from_secs(5);
const MAX_REQUEST_BYTES: usize = 8192;

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn pkce() -> Pkce {
    let verifier = random_string(96);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Pkce { verifier, challenge }
}

/// Random alphanumeric string from the OS CSPRNG (~5.95 bits per char).
pub fn random_string(len: usize) -> String {
    OsRng.sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

/// Comparison whose duration doesn't depend on where the strings differ.
fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn authorize_url(client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> Result<String> {
    let mut url = url::Url::parse(AUTHORIZE_URL)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url.into())
}

/// Loopback-only HTTP listener for the OAuth redirect.
pub struct CallbackServer {
    v4: TcpListener,
    v6: Option<TcpListener>,
    port: u16,
}

impl CallbackServer {
    /// Bind `127.0.0.1:port` (and `[::1]` on the same port when available,
    /// because browsers may resolve `localhost` to IPv6). `port = 0` picks a
    /// free port (tests).
    pub async fn bind(port: u16) -> Result<Self> {
        let v4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
        let port = v4.local_addr()?.port();
        let v6 = TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port))).await.ok();
        Ok(CallbackServer { v4, v6, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://localhost:{}/callback", self.port())
    }

    async fn accept(&self) -> std::io::Result<TcpStream> {
        let (stream, _) = match &self.v6 {
            Some(v6) => tokio::select! {
                r = self.v4.accept() => r?,
                r = v6.accept() => r?,
            },
            None => self.v4.accept().await?,
        };
        Ok(stream)
    }

    /// Serve requests until a callback with the expected `state` arrives,
    /// then run `exchange(code)` and answer the browser with the result.
    pub async fn wait_for_code<F, Fut, T>(&self, expected_state: &str, exchange: F) -> Result<T>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        loop {
            let mut stream = match self.accept().await {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("callback accept failed: {e}");
                    continue;
                }
            };
            let Some(target) = read_request_target(&mut stream).await else {
                let _ = respond(&mut stream, 400, "Ошибка", "Некорректный запрос.").await;
                continue;
            };
            let Ok(url) = url::Url::parse(&format!("http://localhost{target}")) else {
                let _ = respond(&mut stream, 400, "Ошибка", "Некорректный запрос.").await;
                continue;
            };
            if url.path() != "/callback" {
                let _ = respond(&mut stream, 404, "Not found", "").await;
                continue;
            }
            let param = |name: &str| url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned());

            // CSRF check comes first: without the right state nothing else
            // in the request is trusted — not even an `error`.
            let state_ok = param("state").map(|s| constant_time_eq(&s, expected_state)).unwrap_or(false);
            if !state_ok {
                log::warn!("rejected OAuth callback with missing/invalid state");
                let _ = respond(&mut stream, 400, "Ошибка авторизации", "Неверный параметр state.").await;
                continue;
            }
            if let Some(err) = param("error") {
                let desc = param("error_description").unwrap_or_default();
                let _ = respond(&mut stream, 400, "Ошибка авторизации", &format!("{err}: {desc}")).await;
                bail!("SoundCloud вернул ошибку: {err} {desc}");
            }
            let Some(code) = param("code").filter(|c| !c.is_empty()) else {
                let _ = respond(&mut stream, 400, "Ошибка авторизации", "В ответе нет кода.").await;
                bail!("в ответе OAuth нет кода");
            };
            return match exchange(code).await {
                Ok(v) => {
                    let msg = "Вернитесь в приложение — эту вкладку можно закрыть.";
                    let _ = respond(&mut stream, 200, "Авторизация успешна", msg).await;
                    Ok(v)
                }
                Err(e) => {
                    let _ = respond(&mut stream, 500, "Ошибка авторизации", &format!("{e:#}")).await;
                    Err(e)
                }
            };
        }
    }
}

/// Production entry point: port 3000, system browser, 3 minute limit.
pub async fn login(api: &ApiClient, client_id: String, client_secret: String) -> Result<()> {
    let server = CallbackServer::bind(CALLBACK_PORT)
        .await
        .context("порт 3000 занят — закройте приложение, которое его использует")?;
    login_with(api, server, client_id, client_secret, LOGIN_TIMEOUT, |url| {
        log::info!("opening browser for OAuth login");
        open::that_detached(url).context("не удалось открыть браузер")
    })
    .await
}

/// The whole flow with injectable server, deadline and "browser".
pub async fn login_with(
    api: &ApiClient,
    server: CallbackServer,
    client_id: String,
    client_secret: String,
    timeout: Duration,
    open_browser: impl FnOnce(&str) -> Result<()>,
) -> Result<()> {
    let client_id = client_id.trim().to_string();
    let client_secret = client_secret.trim().to_string();
    if client_id.is_empty() {
        bail!("Укажите client_id зарегистрированного приложения SoundCloud");
    }
    let pkce = pkce();
    let state = random_string(32);
    let redirect_uri = server.redirect_uri();
    open_browser(&authorize_url(&client_id, &redirect_uri, &pkce.challenge, &state)?)?;

    let exchange = |code: String| {
        let (client_id, client_secret, verifier, redirect_uri) =
            (client_id.clone(), client_secret.clone(), pkce.verifier.clone(), redirect_uri.clone());
        async move {
            let form = [
                ("grant_type", "authorization_code"),
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("code_verifier", verifier.as_str()),
                ("code", code.as_str()),
            ];
            api.request_token(&form).await
        }
    };
    let result = tokio::time::timeout(timeout, server.wait_for_code(&state, exchange)).await;
    // Release the port right away, whatever the outcome.
    drop(server);
    let session = result.map_err(|_| anyhow!("время ожидания входа истекло"))??;

    api.update_store(|s| {
        s.app_client_id = Some(client_id);
        s.app_client_secret = Some(client_secret);
        s.official = Some(session);
        s.onboarded = true;
    })?;
    api.clear_session_cache();
    Ok(())
}

async fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut buf = vec![0u8; MAX_REQUEST_BYTES];
    let mut len = 0;
    let deadline = tokio::time::Instant::now() + READ_DEADLINE;
    loop {
        let n = tokio::time::timeout_at(deadline, stream.read(&mut buf[len..])).await.ok()?.ok()?;
        if n == 0 {
            break;
        }
        len += n;
        if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") || len == buf.len() {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf[..len]);
    let mut parts = text.lines().next()?.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    parts.next().filter(|t| t.starts_with('/')).map(str::to_owned)
}

async fn respond(stream: &mut TcpStream, status: u16, title: &str, message: &str) -> std::io::Result<()> {
    let ok = status == 200;
    let html = format!(
        r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><title>{title}</title>
<style>
body{{margin:0;height:100vh;display:flex;align-items:center;justify-content:center;background:#121212;color:#fff;font-family:Inter,system-ui,sans-serif}}
.card{{background:#1e1e1e;border-radius:16px;padding:48px 56px;text-align:center;box-shadow:0 10px 40px rgba(0,0,0,.5);max-width:480px}}
.icon{{width:72px;height:72px;border-radius:50%;margin:0 auto 24px;display:flex;align-items:center;justify-content:center;font-size:40px;background:{bg}}}
h1{{margin:0 0 12px;font-size:24px}} p{{margin:0;color:#aaa;line-height:1.5}}
</style></head><body><div class="card"><div class="icon">{icon}</div><h1>{title}</h1><p>{message}</p></div></body></html>"#,
        title = html_escape(title),
        message = html_escape(message),
        icon = if ok { "✓" } else { "✕" },
        bg = if ok { "#ff5500" } else { "#c0392b" },
    );
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        400 => "Bad Request",
        _ => "Internal Server Error",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.shutdown().await
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{client, mock_endpoints};
    use crate::config::AuthStore;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let p = pkce();
        assert!(p.verifier.len() >= 43 && p.verifier.len() <= 128);
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifier.as_bytes()));
        assert_eq!(p.challenge, expected);
        assert!(!p.challenge.contains('='));
    }

    #[test]
    fn state_is_random_and_compared_safely() {
        let a = random_string(32);
        let b = random_string(32);
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(constant_time_eq(&a, &a.clone()));
        assert!(!constant_time_eq(&a, &b));
        assert!(!constant_time_eq(&a, &a[..31]));
    }

    #[test]
    fn authorize_url_has_pkce_params() {
        let u = authorize_url("abc", "http://localhost:3000/callback", "chal", "st").unwrap();
        assert!(u.starts_with(AUTHORIZE_URL));
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3000%2Fcallback"));
    }

    #[tokio::test]
    async fn binds_loopback_only() {
        let server = CallbackServer::bind(0).await.unwrap();
        assert!(server.v4.local_addr().unwrap().ip().is_loopback());
        if let Some(v6) = &server.v6 {
            assert!(v6.local_addr().unwrap().ip().is_loopback());
        }
    }

    fn query(url: &str, key: &str) -> String {
        url::Url::parse(url).unwrap().query_pairs().find(|(k, _)| k == key).unwrap().1.into_owned()
    }

    /// Full cycle on a random port: bad requests are rejected without
    /// aborting, the valid callback is exchanged, the session is saved 0600
    /// and the port is released.
    #[tokio::test]
    async fn oauth_callback_cycle() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=GOOD-CODE"))
            .and(body_string_contains("code_verifier="))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token":"AT-SECRET","refresh_token":"RT-SECRET","expires_in":3600,"scope":"*"
            })))
            .expect(1)
            .mount(&mock)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sc-rust-client");
        let api = client(&mock, &dir, AuthStore::default());
        let server = CallbackServer::bind(0).await.unwrap();
        let port = server.port();
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();

        let browser = async move {
            let auth_url = rx.await.unwrap();
            assert_eq!(query(&auth_url, "redirect_uri"), format!("http://localhost:{port}/callback"));
            assert_eq!(query(&auth_url, "code_challenge_method"), "S256");
            let state = query(&auth_url, "state");
            let http = reqwest::Client::new();
            let get = |q: String| http.get(format!("http://127.0.0.1:{port}{q}")).send();

            // Forged callbacks (CSRF) are refused, and do not end the flow.
            let r = get("/callback?code=EVIL&state=WRONG".into()).await.unwrap();
            assert_eq!(r.status(), 400);
            let r = get("/callback?error=access_denied&state=WRONG".into()).await.unwrap();
            assert_eq!(r.status(), 400);
            let r = get("/callback?code=EVIL".into()).await.unwrap();
            assert_eq!(r.status(), 400);
            let r = get("/favicon.ico".into()).await.unwrap();
            assert_eq!(r.status(), 404);
            // Garbage on the socket is handled too.
            let mut raw = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            raw.write_all(b"\x00\x01garbage\r\n\r\n").await.unwrap();
            let mut sink = Vec::new();
            let _ = raw.read_to_end(&mut sink).await;

            let r = get(format!("/callback?code=GOOD-CODE&state={state}")).await.unwrap();
            assert_eq!(r.status(), 200);
            assert!(r.text().await.unwrap().contains("Авторизация успешна"));
        };
        let flow = login_with(&api, server, "app-id".into(), "app-secret".into(), Duration::from_secs(10), |url| {
            tx.send(url.to_owned()).map_err(|_| anyhow!("send"))
        });
        let (res, ()) = tokio::join!(flow, browser);
        res.unwrap();

        let session = api.store().official.unwrap();
        assert_eq!(session.access_token, "AT-SECRET");
        let file = dir.join("auth.json");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(std::fs::read_to_string(&file).unwrap().contains("RT-SECRET"));
        // Port is free again.
        TcpListener::bind(("127.0.0.1", port)).await.expect("port released after login");
    }

    #[tokio::test]
    async fn times_out_and_releases_port() {
        let mock = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let api = ApiClient::with_endpoints(mock_endpoints(&mock.uri()), tmp.path().to_path_buf()).unwrap();
        let server = CallbackServer::bind(0).await.unwrap();
        let port = server.port();
        let started = std::time::Instant::now();
        let e = login_with(&api, server, "id".into(), "s".into(), Duration::from_millis(300), |_| Ok(()))
            .await
            .unwrap_err();
        assert!(format!("{e}").contains("истекло"));
        assert!(started.elapsed() < Duration::from_secs(3));
        TcpListener::bind(("127.0.0.1", port)).await.expect("port released after timeout");
        assert!(api.store().official.is_none());
    }

    #[tokio::test]
    async fn provider_error_with_valid_state_aborts_cleanly() {
        let mock = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let api = ApiClient::with_endpoints(mock_endpoints(&mock.uri()), tmp.path().to_path_buf()).unwrap();
        let server = CallbackServer::bind(0).await.unwrap();
        let port = server.port();
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        let browser = async move {
            let state = query(&rx.await.unwrap(), "state");
            let r = reqwest::get(format!("http://127.0.0.1:{port}/callback?error=access_denied&state={state}"))
                .await
                .unwrap();
            assert_eq!(r.status(), 400);
        };
        let flow = login_with(&api, server, "id".into(), "s".into(), Duration::from_secs(10), |u| {
            tx.send(u.to_owned()).map_err(|_| anyhow!("send"))
        });
        let (res, ()) = tokio::join!(flow, browser);
        assert!(format!("{:#}", res.unwrap_err()).contains("access_denied"));
        assert!(!tmp.path().join("auth.json").exists());
    }
}
