//! Password login with server-side sessions.
//!
//! The browser holds a random 256-bit token in an HttpOnly, SameSite=Strict cookie. The
//! database stores only its SHA-256, so a leaked database can't be used to sign in.
//! Sessions slide: each day of use pushes the expiry out to 30 days from now.

use std::sync::LazyLock;

use anyhow::{Context, anyhow};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::{
    Json, Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, header, request::Parts},
    routing::post,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

pub const COOKIE: &str = "kanban_session";
const SESSION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub avatar_color: String,
}

/// Extractor: the signed-in user. Rejects with 401 when there is no valid session.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(COOKIE).ok_or(AppError::Unauthorized)?.value();
        let hash = token_hash(token);

        let row: Option<(i64, String, String, String, i64)> = sqlx::query_as(
            "SELECT u.id, u.username, u.display_name, u.avatar_color,
                    s.expires_at < datetime('now', '+29 days') AS stale
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > datetime('now')",
        )
        .bind(&hash)
        .fetch_optional(&state.db)
        .await?;
        let (id, username, display_name, avatar_color, stale) =
            row.ok_or(AppError::Unauthorized)?;

        if stale != 0 {
            sqlx::query("UPDATE sessions SET expires_at = datetime('now', ?) WHERE token_hash = ?")
                .bind(format!("+{SESSION_DAYS} days"))
                .bind(&hash)
                .execute(&state.db)
                .await?;
        }

        Ok(Self(User {
            id,
            username,
            display_name,
            avatar_color,
        }))
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<LoginBody>,
) -> AppResult<(CookieJar, Json<crate::prefs::Me>)> {
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE username = ?")
            .bind(body.username.trim())
            .fetch_optional(&state.db)
            .await?;

    // Verify against a dummy hash for unknown users so both paths take the same time.
    let (user_id, stored) = match row {
        Some((id, h)) => (Some(id), h),
        None => (None, DUMMY_HASH.clone()),
    };
    let ok = verify_password(body.password, stored).await?;
    let user_id = match (ok, user_id) {
        (true, Some(id)) => id,
        _ => return Err(AppError::bad("wrong username or password")),
    };

    let token = new_token()?;
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(200).collect::<String>());
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at, user_agent)
         VALUES (?, ?, datetime('now', ?), ?)",
    )
    .bind(token_hash(&token))
    .bind(user_id)
    .bind(format!("+{SESSION_DAYS} days"))
    .bind(user_agent)
    .execute(&state.db)
    .await?;

    let me = crate::prefs::me_for(&state.db, get_user(&state.db, user_id).await?).await?;
    let cookie = Cookie::build((COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Strict)
        .path("/")
        // The server-side expiry is authoritative; keep the cookie around as long as browsers allow.
        .max_age(time_days(400))
        .build();
    Ok((jar.add(cookie), Json(me)))
}

async fn logout(State(state): State<AppState>, jar: CookieJar) -> AppResult<CookieJar> {
    if let Some(c) = jar.get(COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash(c.value()))
            .execute(&state.db)
            .await?;
    }
    Ok(jar.remove(Cookie::build(COOKIE).path("/")))
}

pub async fn get_user(db: &SqlitePool, id: i64) -> AppResult<User> {
    Ok(
        sqlx::query_as("SELECT id, username, display_name, avatar_color FROM users WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await?,
    )
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow!("hashing password: {e}"))
}

async fn verify_password(password: String, stored: String) -> anyhow::Result<bool> {
    // argon2 is deliberately slow; keep it off the async worker threads.
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&stored).map_err(|e| anyhow!("bad stored hash: {e}"))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .context("password check panicked")?
}

static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash_password("not-a-real-password").expect("hashing dummy password"));

fn new_token() -> anyhow::Result<String> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(|e| anyhow!("reading system randomness: {e}"))?;
    Ok(hex::encode(buf))
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn time_days(days: i64) -> time::Duration {
    time::Duration::days(days)
}

pub async fn purge_expired_sessions(db: &SqlitePool) -> sqlx::Result<u64> {
    Ok(
        sqlx::query("DELETE FROM sessions WHERE expires_at <= datetime('now')")
            .execute(db)
            .await?
            .rows_affected(),
    )
}
