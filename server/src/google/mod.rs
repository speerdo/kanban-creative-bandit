//! Google integration (docs/adr/0002-google-integration.md): one OAuth connection per person,
//! a plain REST client for the handful of Calendar endpoints we call, and refresh tokens
//! encrypted at rest.
//!
//! Configured from the environment: `KANBAN_GOOGLE_CLIENT_ID` and `KANBAN_GOOGLE_CLIENT_SECRET`
//! (from /etc/kanban/google.env), and a 32-byte key file, passed by systemd as the `token-key`
//! credential or named by `KANBAN_TOKEN_KEY_FILE`. Without them the app runs with Google off.

pub mod calendar;
pub mod drive;
pub mod gmail;
mod oauth;
mod routes;
pub mod sync;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{Context, anyhow, bail};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use reqwest::{Method, StatusCode, Url};
use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::error::AppError;

pub use routes::router;

/// Where the requests go. Tests point these at a stand-in server.
#[derive(Clone)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub revoke: String,
    pub calendar: String,
    pub gmail: String,
    pub drive: String,
}

impl Endpoints {
    /// Every endpoint under one base URL (a stand-in server for tests and development).
    pub fn at(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            auth: format!("{base}/auth"),
            token: format!("{base}/token"),
            revoke: format!("{base}/revoke"),
            calendar: format!("{base}/cal"),
            gmail: format!("{base}/gmail"),
            drive: format!("{base}/drive"),
        }
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
            calendar: "https://www.googleapis.com/calendar/v3".into(),
            gmail: "https://gmail.googleapis.com/gmail/v1".into(),
            drive: "https://www.googleapis.com/drive/v3".into(),
        }
    }
}

pub struct Config {
    pub client_id: String,
    pub client_secret: String,
    pub key: [u8; 32],
    pub endpoints: Endpoints,
}

impl Config {
    /// `None` (with the reason logged) when Google isn't set up on this server.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let id = std::env::var("KANBAN_GOOGLE_CLIENT_ID").unwrap_or_default();
        let secret = std::env::var("KANBAN_GOOGLE_CLIENT_SECRET").unwrap_or_default();
        if id.trim().is_empty() || secret.trim().is_empty() {
            tracing::info!(
                "Google integration off: no client id/secret (see docs/google-setup.md)"
            );
            return Ok(None);
        }
        let key_file = match std::env::var_os("CREDENTIALS_DIRECTORY") {
            Some(dir) => PathBuf::from(dir).join("token-key"),
            None => match std::env::var_os("KANBAN_TOKEN_KEY_FILE") {
                Some(f) => f.into(),
                None => {
                    tracing::warn!(
                        "Google integration off: no token key (KANBAN_TOKEN_KEY_FILE or the systemd \
                         token-key credential, see docs/google-setup.md)"
                    );
                    return Ok(None);
                }
            },
        };
        let raw = std::fs::read(&key_file)
            .with_context(|| format!("reading the token key {}", key_file.display()))?;
        let key: [u8; 32] = raw.as_slice().try_into().map_err(|_| {
            anyhow!(
                "the token key {} must be exactly 32 bytes (it is {}); create it with \
                 head -c 32 /dev/urandom",
                key_file.display(),
                raw.len()
            )
        })?;
        // Development only: talk to a stand-in Google instead of the real one.
        let endpoints = match std::env::var("KANBAN_GOOGLE_BASE_URL") {
            Ok(base) if !base.is_empty() => {
                tracing::warn!(%base, "using a stand-in Google server");
                Endpoints::at(&base)
            }
            _ => Endpoints::default(),
        };
        Ok(Some(Self {
            client_id: id.trim().into(),
            client_secret: secret.trim().into(),
            key,
            endpoints,
        }))
    }
}

/// Why a Google call failed, in terms the UI can show.
#[derive(Debug)]
pub enum GoogleError {
    NotConfigured,
    NotConnected,
    /// The refresh token no longer works: access was removed in the Google account.
    Revoked,
    NotFound,
    /// A sync token expired; do a full sync.
    Gone,
    Api(StatusCode, String),
    Other(anyhow::Error),
}

impl std::fmt::Display for GoogleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => {
                f.write_str("Google isn't set up on the server yet (see docs/google-setup.md).")
            }
            Self::NotConnected => f.write_str("Connect your Google account first."),
            Self::Revoked => f.write_str("Google access was revoked. Reconnect."),
            Self::NotFound => f.write_str("Google couldn't find that calendar or event."),
            Self::Gone => f.write_str("Google asked for a full resync."),
            Self::Api(status, msg) => write!(f, "Google said: {msg} ({})", status.as_u16()),
            Self::Other(e) => write!(f, "Couldn't reach Google: {e:#}"),
        }
    }
}

impl From<reqwest::Error> for GoogleError {
    fn from(e: reqwest::Error) -> Self {
        Self::Other(e.into())
    }
}

impl From<sqlx::Error> for GoogleError {
    fn from(e: sqlx::Error) -> Self {
        Self::Other(e.into())
    }
}

impl From<anyhow::Error> for GoogleError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

impl From<GoogleError> for AppError {
    fn from(e: GoogleError) -> Self {
        match e {
            GoogleError::Other(e) => AppError::Upstream(format!("Couldn't reach Google: {e:#}")),
            GoogleError::Api(..) => AppError::Upstream(e.to_string()),
            e => AppError::Conflict(e.to_string()),
        }
    }
}

pub type GResult<T> = Result<T, GoogleError>;

/// An OAuth attempt waiting for its pasted-back URL.
struct Pending {
    state: String,
    verifier: String,
    started: Instant,
}

pub struct Google {
    cfg: Option<Config>,
    http: reqwest::Client,
    pending: Mutex<HashMap<i64, Pending>>,
    /// Access tokens by user, with their expiry.
    tokens: Mutex<HashMap<i64, (String, Instant)>>,
    /// Syncs and pushes run one at a time, so two never interleave writes for the same events.
    pub busy: tokio::sync::Mutex<()>,
}

impl Google {
    pub fn new(cfg: Option<Config>) -> anyhow::Result<Self> {
        // rustls needs a crypto provider; ring is the light one. Ignore "already installed".
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("kanban/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("building the HTTP client")?;
        Ok(Self {
            cfg,
            http,
            pending: Mutex::default(),
            tokens: Mutex::default(),
            busy: tokio::sync::Mutex::new(()),
        })
    }

    pub fn configured(&self) -> bool {
        self.cfg.is_some()
    }

    fn cfg(&self) -> GResult<&Config> {
        self.cfg.as_ref().ok_or(GoogleError::NotConfigured)
    }

    // ---- refresh tokens at rest ----------------------------------------------------------

    /// Encrypts a refresh token, bound to its user so a row copied to another user won't open.
    fn seal(&self, user: i64, token: &str) -> GResult<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new((&self.cfg()?.key).into());
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|e| anyhow!("reading system randomness: {e}"))?;
        let aad = user.to_be_bytes();
        let sealed = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: token.as_bytes(),
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow!("encrypting the refresh token"))?;
        Ok([nonce.as_slice(), &sealed].concat())
    }

    fn open(&self, user: i64, blob: &[u8]) -> GResult<String> {
        if blob.len() < 24 {
            return Err(anyhow!("stored refresh token is truncated").into());
        }
        let cipher = XChaCha20Poly1305::new((&self.cfg()?.key).into());
        let (nonce, sealed) = blob.split_at(24);
        let nonce: [u8; 24] = nonce.try_into().expect("split at 24");
        let aad = user.to_be_bytes();
        let plain = cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: sealed,
                    aad: &aad,
                },
            )
            // A different key file than the one the token was stored with.
            .map_err(|_| GoogleError::Revoked)?;
        Ok(String::from_utf8(plain).map_err(|_| anyhow!("stored refresh token isn't text"))?)
    }

    // ---- access tokens -------------------------------------------------------------------

    async fn access_token(&self, db: &SqlitePool, user: i64) -> GResult<String> {
        if let Some((token, expires)) = self.tokens.lock().unwrap().get(&user)
            && *expires > Instant::now() + Duration::from_secs(60)
        {
            return Ok(token.clone());
        }
        let blob: Vec<u8> =
            sqlx::query_scalar("SELECT refresh_token_enc FROM google_accounts WHERE user_id = ?")
                .bind(user)
                .fetch_optional(db)
                .await?
                .ok_or(GoogleError::NotConnected)?;
        let refresh = self.open(user, &blob)?;
        let cfg = self.cfg()?;
        let res = self
            .http
            .post(&cfg.endpoints.token)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
                ("client_id", cfg.client_id.as_str()),
                ("client_secret", cfg.client_secret.as_str()),
            ])
            .send()
            .await?;
        let token: TokenResponse = token_response(res).await?;
        self.remember(user, &token);
        Ok(token.access_token)
    }

    fn remember(&self, user: i64, token: &TokenResponse) {
        let expires = Instant::now() + Duration::from_secs(token.expires_in.unwrap_or(3600));
        self.tokens
            .lock()
            .unwrap()
            .insert(user, (token.access_token.clone(), expires));
    }

    #[cfg(test)]
    pub fn forget_access_token(&self, user: i64) {
        self.tokens.lock().unwrap().remove(&user);
    }

    fn forget(&self, user: i64) {
        self.tokens.lock().unwrap().remove(&user);
        self.pending.lock().unwrap().remove(&user);
    }

    // ---- API calls -----------------------------------------------------------------------

    /// A Calendar API URL from path segments (each one percent-encoded, since calendar ids
    /// contain `@` and `#`).
    fn calendar_url(&self, segments: &[&str]) -> GResult<Url> {
        api_url(&self.cfg()?.endpoints.calendar, segments)
    }

    fn gmail_url(&self, segments: &[&str]) -> GResult<Url> {
        api_url(&self.cfg()?.endpoints.gmail, segments)
    }

    fn drive_url(&self, segments: &[&str]) -> GResult<Url> {
        api_url(&self.cfg()?.endpoints.drive, segments)
    }

    /// One authorized JSON call. `Ok(None)` for an empty response (a delete).
    async fn call(
        &self,
        db: &SqlitePool,
        user: i64,
        method: Method,
        url: Url,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> GResult<Option<Value>> {
        let token = self.access_token(db, user).await?;
        let mut req = self
            .http
            .request(method, url)
            .bearer_auth(token)
            .query(query);
        if let Some(body) = body {
            req = req.json(body);
        }
        let res = req.send().await?;
        let status = res.status();
        if status == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        if status.is_success() {
            return Ok(Some(res.json().await?));
        }
        let text = res.text().await.unwrap_or_default();
        Err(match status {
            StatusCode::NOT_FOUND => GoogleError::NotFound,
            StatusCode::GONE => GoogleError::Gone,
            StatusCode::UNAUTHORIZED => {
                // A stale access token; the next call fetches a fresh one.
                self.tokens.lock().unwrap().remove(&user);
                GoogleError::Api(status, google_message(&text))
            }
            _ => GoogleError::Api(status, google_message(&text)),
        })
    }
}

fn api_url(base: &str, segments: &[&str]) -> GResult<Url> {
    let mut url = Url::parse(base).map_err(|e| anyhow!("bad Google endpoint {base}: {e}"))?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("bad Google endpoint {base}"))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

/// Optional parts of the connection, each asked for only when it's turned on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Feature {
    Gmail,
    Drive,
}

impl Feature {
    pub fn scope(self) -> &'static str {
        match self {
            Self::Gmail => "https://www.googleapis.com/auth/gmail.modify",
            Self::Drive => "https://www.googleapis.com/auth/drive.metadata.readonly",
        }
    }
}

/// Whether `user` granted `feature`'s scope.
pub async fn granted(db: &SqlitePool, user: i64, feature: Feature) -> sqlx::Result<bool> {
    let scopes: Option<String> =
        sqlx::query_scalar("SELECT granted_scopes FROM google_accounts WHERE user_id = ?")
            .bind(user)
            .fetch_optional(db)
            .await?;
    Ok(scopes.is_some_and(|s| s.split_whitespace().any(|x| x == feature.scope())))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    id_token: Option<String>,
}

/// Parses a token endpoint answer; `invalid_grant` means the grant is gone.
async fn token_response(res: reqwest::Response) -> GResult<TokenResponse> {
    let status = res.status();
    if status.is_success() {
        return Ok(res.json().await?);
    }
    let text = res.text().await.unwrap_or_default();
    let error: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    match error["error"].as_str() {
        Some("invalid_grant") => Err(GoogleError::Revoked),
        _ => Err(GoogleError::Api(status, google_message(&text))),
    }
}

/// The human part of a Google error body.
fn google_message(body: &str) -> String {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let msg = v["error"]["message"]
        .as_str()
        .or(v["error_description"].as_str())
        .or(v["error"].as_str())
        .unwrap_or(body);
    msg.chars().take(300).collect()
}

/// Random bytes as URL-safe base64 (PKCE verifiers, OAuth state).
fn random_token(bytes: usize) -> GResult<String> {
    use base64::Engine as _;
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| anyhow!("reading system randomness: {e}"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

/// The claims we need from Google's id token. It came straight from Google's token endpoint
/// over TLS, so the payload is read without checking the signature.
#[derive(Debug, Deserialize)]
struct IdClaims {
    sub: String,
    email: Option<String>,
}

fn id_claims(id_token: &str) -> anyhow::Result<IdClaims> {
    use base64::Engine as _;
    let payload = id_token.split('.').nth(1).context("malformed id token")?;
    let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .context("malformed id token")?;
    let claims: IdClaims = serde_json::from_slice(&json).context("malformed id token")?;
    if claims.sub.is_empty() {
        bail!("id token has no subject");
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn google() -> Google {
        Google::new(Some(Config {
            client_id: "id".into(),
            client_secret: "secret".into(),
            key: [7; 32],
            endpoints: Endpoints::default(),
        }))
        .unwrap()
    }

    #[test]
    fn refresh_tokens_round_trip_and_are_bound_to_their_user() {
        let g = google();
        let blob = g.seal(1, "1//refresh").unwrap();
        assert!(!blob.windows(9).any(|w| w == b"1//refresh"[..9].as_ref()));
        assert_eq!(g.open(1, &blob).unwrap(), "1//refresh");
        assert!(matches!(g.open(2, &blob), Err(GoogleError::Revoked)));

        let other_key = Google::new(Some(Config {
            key: [8; 32],
            ..Config {
                client_id: "id".into(),
                client_secret: "secret".into(),
                key: [0; 32],
                endpoints: Endpoints::default(),
            }
        }))
        .unwrap();
        assert!(matches!(
            other_key.open(1, &blob),
            Err(GoogleError::Revoked)
        ));
    }

    #[test]
    fn calendar_ids_are_escaped_in_urls() {
        let url = google()
            .calendar_url(&[
                "calendars",
                "family#contacts@group.v.calendar.google.com",
                "events",
            ])
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://www.googleapis.com/calendar/v3/calendars/family%23contacts@group.v.calendar.google.com/events"
        );
    }
}
