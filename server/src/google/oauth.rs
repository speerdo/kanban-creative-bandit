//! Connecting a Google account: Desktop-client OAuth with PKCE and a loopback redirect that
//! nothing listens on. The person pastes the address their browser ended up on back into
//! the app (the server is LAN-only on plain HTTP, so it can't be a redirect URI itself).

use std::time::{Duration, Instant};

use reqwest::Url;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::{
    Feature, GResult, Google, GoogleError, Pending, id_claims, random_token, token_response,
};
use crate::error::AppError;

/// Desktop clients may redirect to any loopback port. Nothing listens here; the page fails to
/// load and the code stays in the address bar.
pub const REDIRECT_URI: &str = "http://127.0.0.1:8642/";

const SCOPES: &str = "openid email https://www.googleapis.com/auth/calendar";

/// How long an attempt waits for its pasted URL.
const PENDING_FOR: Duration = Duration::from_secs(30 * 60);

impl Google {
    /// Starts a connection for `user`: the Google consent page to open. `features` adds their
    /// scopes; scopes granted before are kept (incremental authorization).
    pub fn start(&self, user: i64, features: &[Feature]) -> GResult<String> {
        let scopes = std::iter::once(SCOPES)
            .chain(features.iter().map(|f| f.scope()))
            .collect::<Vec<_>>()
            .join(" ");
        use base64::Engine as _;
        let cfg = self.cfg()?;
        let verifier = random_token(48)?;
        let state = random_token(16)?;
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        let url = Url::parse_with_params(
            &cfg.endpoints.auth,
            [
                ("client_id", cfg.client_id.as_str()),
                ("redirect_uri", REDIRECT_URI),
                ("response_type", "code"),
                ("scope", scopes.as_str()),
                ("state", &state),
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
                // A refresh token every time, even when reconnecting.
                ("access_type", "offline"),
                ("prompt", "consent"),
                ("include_granted_scopes", "true"),
            ],
        )
        .map_err(|e| anyhow::anyhow!("bad auth endpoint: {e}"))?;
        self.pending.lock().unwrap().insert(
            user,
            Pending {
                state,
                verifier,
                started: Instant::now(),
            },
        );
        Ok(url.into())
    }

    /// Finishes the connection from the pasted address. Stores the encrypted refresh token and
    /// returns the account's email.
    pub async fn finish(
        &self,
        db: &SqlitePool,
        user: i64,
        pasted: &str,
    ) -> Result<String, AppError> {
        let url = Url::parse(pasted.trim()).map_err(|_| {
            AppError::bad("Paste the whole address from the browser's address bar (it starts with http://127.0.0.1).")
        })?;
        let param = |name: &str| {
            url.query_pairs()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.into_owned())
        };
        if let Some(error) = param("error") {
            return Err(AppError::bad(match error.as_str() {
                "access_denied" => {
                    "Google access wasn't granted. Click Connect Google to try again.".into()
                }
                other => format!("Google refused the connection: {other}"),
            }));
        }
        let code = param("code").ok_or_else(|| {
            AppError::bad(
                "That address has no code in it. Copy it again after approving on Google's page.",
            )
        })?;

        let pending = {
            let mut all = self.pending.lock().unwrap();
            match all.get(&user) {
                Some(p) if p.started.elapsed() >= PENDING_FOR => None,
                // A stale or mistyped paste leaves the attempt open for the right one.
                Some(p) if param("state").as_deref() != Some(p.state.as_str()) => {
                    return Err(AppError::bad(
                        "That address is from a different attempt. Use the newest Google tab, or click Connect Google again.",
                    ));
                }
                _ => all.remove(&user),
            }
        }
        .ok_or_else(|| AppError::bad("This connection attempt expired. Click Connect Google again."))?;

        let cfg = self.cfg()?;
        let res = self
            .http
            .post(&cfg.endpoints.token)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", REDIRECT_URI),
                ("client_id", cfg.client_id.as_str()),
                ("client_secret", cfg.client_secret.as_str()),
                ("code_verifier", pending.verifier.as_str()),
            ])
            .send()
            .await
            .map_err(GoogleError::from)?;
        let token = match token_response(res).await {
            Err(GoogleError::Revoked) => {
                return Err(AppError::bad(
                    "Google says that code was already used or has expired. Click Connect Google again.",
                ));
            }
            other => other?,
        };
        let claims = token
            .id_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Google sent no id token"))
            .and_then(id_claims)
            .map_err(GoogleError::from)?;
        let scopes = token.scope.clone().unwrap_or_default();
        if !scopes.contains("https://www.googleapis.com/auth/calendar") {
            return Err(AppError::bad(
                "Calendar access wasn't granted. Click Connect Google again and leave the Calendar box ticked.",
            ));
        }

        let existing: Option<String> =
            sqlx::query_scalar("SELECT google_sub FROM google_accounts WHERE user_id = ?")
                .bind(user)
                .fetch_optional(db)
                .await?;
        let sealed = match &token.refresh_token {
            Some(refresh) => self.seal(user, refresh)?,
            None => {
                return Err(AppError::bad(
                    "Google didn't send a long-lived token. Remove Kanban at \
                     myaccount.google.com/permissions, then connect again.",
                ));
            }
        };
        let email = claims.email.unwrap_or_else(|| "(no email)".into());

        let mut tx = db.begin().await?;
        if existing.as_deref().is_some_and(|sub| sub != claims.sub) {
            // A different Google account: nothing from the old one applies any more.
            forget_calendars(&mut tx, user).await?;
        }
        sqlx::query(
            "INSERT INTO google_accounts (user_id, google_sub, email, granted_scopes, refresh_token_enc)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (user_id) DO UPDATE SET google_sub = excluded.google_sub,
                 email = excluded.email, granted_scopes = excluded.granted_scopes,
                 refresh_token_enc = excluded.refresh_token_enc, last_error = NULL,
                 connected_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )
        .bind(user)
        .bind(&claims.sub)
        .bind(&email)
        .bind(&scopes)
        .bind(sealed)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.remember(user, &token);
        Ok(email)
    }

    /// Revokes access at Google (best effort) and deletes everything stored for `user`.
    /// Events already pushed stay in Google Calendar.
    pub async fn disconnect(&self, db: &SqlitePool, user: i64) -> Result<(), AppError> {
        let blob: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT refresh_token_enc FROM google_accounts WHERE user_id = ?")
                .bind(user)
                .fetch_optional(db)
                .await?;
        if let (Some(blob), Ok(cfg)) = (blob, self.cfg())
            && let Ok(refresh) = self.open(user, &blob)
        {
            let revoked = self
                .http
                .post(&cfg.endpoints.revoke)
                .form(&[("token", refresh.as_str())])
                .send()
                .await;
            if let Err(e) = revoked {
                tracing::warn!(user, "revoking the Google token: {e}");
            }
        }
        let mut tx = db.begin().await?;
        forget_calendars(&mut tx, user).await?;
        sqlx::query("DELETE FROM google_accounts WHERE user_id = ?")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.forget(user);
        Ok(())
    }
}

async fn forget_calendars(conn: &mut sqlx::SqliteConnection, user: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM task_events WHERE user_id = ?")
        .bind(user)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM google_calendars WHERE user_id = ?")
        .bind(user)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
