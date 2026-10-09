//! `/api/integrations/google…`: connect and disconnect, choose calendars, sync, push.
//! Everything here is about the signed-in person's own Google account.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Feature, gmail, granted, oauth::REDIRECT_URI, sync};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/integrations/google", get(status).delete(disconnect))
        .route("/integrations/google/start", post(start))
        .route("/integrations/google/finish", post(finish))
        .route(
            "/integrations/google/calendars",
            get(list_calendars).put(choose_calendars),
        )
        .route("/integrations/google/sync", post(sync_now))
        .route("/integrations/google/gmail", axum::routing::put(set_gmail))
        .route("/integrations/google/push", get(push_count).post(push))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ChosenCalendar {
    google_calendar_id: String,
    summary: String,
    color: Option<String>,
    role: String,
    synced_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct Status {
    /// Google is set up on the server (client id and key present).
    configured: bool,
    connected: bool,
    email: Option<String>,
    connected_at: Option<String>,
    last_sync_at: Option<String>,
    last_push_at: Option<String>,
    last_error: Option<String>,
    calendars: Vec<ChosenCalendar>,
    /// Changes the next push would send.
    pending: usize,
    /// Optional scopes granted: `gmail`, `drive`.
    features: Vec<Feature>,
    gmail_enabled: bool,
    gmail_project_id: Option<i64>,
    redirect_uri: &'static str,
}

#[derive(sqlx::FromRow)]
struct Account {
    email: String,
    connected_at: String,
    last_sync_at: Option<String>,
    last_push_at: Option<String>,
    last_error: Option<String>,
    gmail_enabled: bool,
    gmail_project_id: Option<i64>,
}

async fn current(state: &AppState, user: i64) -> AppResult<Status> {
    let account: Option<Account> = sqlx::query_as(
        "SELECT email, connected_at, last_sync_at, last_push_at, last_error, gmail_enabled,
                gmail_project_id
         FROM google_accounts WHERE user_id = ?",
    )
    .bind(user)
    .fetch_optional(&state.db)
    .await?;
    let calendars = sqlx::query_as(
        "SELECT google_calendar_id, summary, color, role, synced_at FROM google_calendars
         WHERE user_id = ? ORDER BY role = 'overlay', summary",
    )
    .bind(user)
    .fetch_all(&state.db)
    .await?;
    let pending = sync::pending(&state.db, user).await?;
    let mut features = vec![];
    for f in [Feature::Gmail, Feature::Drive] {
        if granted(&state.db, user, f).await? {
            features.push(f);
        }
    }
    Ok(Status {
        configured: state.google.configured(),
        connected: account.is_some(),
        email: account.as_ref().map(|a| a.email.clone()),
        connected_at: account.as_ref().map(|a| a.connected_at.clone()),
        last_sync_at: account.as_ref().and_then(|a| a.last_sync_at.clone()),
        last_push_at: account.as_ref().and_then(|a| a.last_push_at.clone()),
        gmail_enabled: account.as_ref().is_some_and(|a| a.gmail_enabled),
        gmail_project_id: account.as_ref().and_then(|a| a.gmail_project_id),
        last_error: account.and_then(|a| a.last_error),
        features,
        calendars,
        pending,
        redirect_uri: REDIRECT_URI,
    })
}

async fn status(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> AppResult<Json<Status>> {
    Ok(Json(current(&state, me.id).await?))
}

#[derive(Deserialize, Default)]
struct StartBody {
    #[serde(default)]
    features: Vec<Feature>,
}

async fn start(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    body: Option<Json<StartBody>>,
) -> AppResult<Json<Value>> {
    let features = body.map(|b| b.0.features).unwrap_or_default();
    let auth_url = state.google.start(me.id, &features)?;
    Ok(Json(json!({ "auth_url": auth_url })))
}

#[derive(Deserialize)]
struct FinishBody {
    redirected_url: String,
}

async fn finish(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<FinishBody>,
) -> AppResult<Json<Status>> {
    let email = state
        .google
        .finish(&state.db, me.id, &body.redirected_url)
        .await?;
    tracing::info!(user = me.id, %email, "connected Google");
    Ok(Json(current(&state, me.id).await?))
}

async fn disconnect(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> AppResult<StatusCode> {
    // Wait for a running sync so it can't write rows back after we delete them.
    let _one_at_a_time = state.google.busy.lock().await;
    state.google.disconnect(&state.db, me.id).await?;
    state
        .events
        .send("calendar.synced", me.id, json!({ "user_id": me.id }));
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct CalendarOption {
    id: String,
    summary: String,
    color: Option<String>,
    primary: bool,
    writable: bool,
    /// How we use it now: `tasks`, `overlay`, or null.
    role: Option<String>,
}

async fn list_calendars(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> AppResult<Json<Vec<CalendarOption>>> {
    let chosen: Vec<(String, String)> =
        sqlx::query_as("SELECT google_calendar_id, role FROM google_calendars WHERE user_id = ?")
            .bind(me.id)
            .fetch_all(&state.db)
            .await?;
    let mut calendars: Vec<CalendarOption> = state
        .google
        .calendars(&state.db, me.id)
        .await?
        .into_iter()
        .map(|c| CalendarOption {
            role: chosen
                .iter()
                .find(|(id, _)| *id == c.id)
                .map(|(_, r)| r.clone()),
            summary: c.name().to_string(),
            primary: c.primary,
            writable: c.writable(),
            color: c.background_color,
            id: c.id,
        })
        .collect();
    calendars.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then_with(|| a.summary.to_lowercase().cmp(&b.summary.to_lowercase()))
    });
    Ok(Json(calendars))
}

/// The id to use for "make me a new Kanban calendar".
const NEW_CALENDAR: &str = "new";

#[derive(Deserialize)]
struct ChooseBody {
    /// The calendar Push to Google writes to: a calendar id, `"new"`, or null for none.
    tasks: Option<String>,
    /// Calendars to show, read-only, in the Calendar view.
    #[serde(default)]
    overlays: Vec<String>,
}

async fn choose_calendars(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<ChooseBody>,
) -> AppResult<Json<Status>> {
    let g = &state.google;
    let available = g.calendars(&state.db, me.id).await?;
    let find = |id: &str| available.iter().find(|c| c.id == id);

    let tasks = match body.tasks.as_deref() {
        None => None,
        Some(NEW_CALENDAR) => {
            let id = g.create_calendar(&state.db, me.id, "Kanban").await?;
            Some((id, "Kanban".to_string(), None))
        }
        Some(id) => {
            let cal = find(id)
                .ok_or_else(|| AppError::bad("that calendar isn't in your Google account"))?;
            if !cal.writable() {
                return Err(AppError::bad(format!(
                    "you can't add events to “{}”; pick a calendar you own",
                    cal.name()
                )));
            }
            Some((
                cal.id.clone(),
                cal.name().to_string(),
                cal.background_color.clone(),
            ))
        }
    };
    let mut overlays = vec![];
    for id in &body.overlays {
        if tasks.as_ref().is_some_and(|(t, _, _)| t == id) {
            continue; // the tasks calendar shows as tasks, not twice
        }
        let cal =
            find(id).ok_or_else(|| AppError::bad("that calendar isn't in your Google account"))?;
        overlays.push((
            cal.id.clone(),
            cal.name().to_string(),
            cal.background_color.clone(),
        ));
    }

    {
        let _one_at_a_time = g.busy.lock().await;
        let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
        let old_tasks: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT google_calendar_id, sync_token FROM google_calendars WHERE user_id = ? AND role = 'tasks'",
        )
        .bind(me.id)
        .fetch_optional(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM google_calendars WHERE user_id = ?")
            .bind(me.id)
            .execute(&mut *tx)
            .await?;
        if let Some((id, summary, color)) = &tasks {
            // Keep the sync token when the tasks calendar stays the same.
            let token = old_tasks.filter(|(old, _)| old == id).and_then(|(_, t)| t);
            sqlx::query(
                "INSERT INTO google_calendars (user_id, google_calendar_id, summary, color, role, sync_token)
                 VALUES (?, ?, ?, ?, 'tasks', ?)",
            )
            .bind(me.id)
            .bind(id)
            .bind(summary)
            .bind(color)
            .bind(token)
            .execute(&mut *tx)
            .await?;
        }
        for (id, summary, color) in &overlays {
            sqlx::query(
                "INSERT INTO google_calendars (user_id, google_calendar_id, summary, color, role)
                 VALUES (?, ?, ?, ?, 'overlay')",
            )
            .bind(me.id)
            .bind(id)
            .bind(summary)
            .bind(color)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }

    // Fill the Calendar view now rather than in up to 15 minutes. A failure is recorded in
    // the status (last_error), and the choice itself is saved either way.
    if let Err(e) = sync::pull(&state, me.id).await {
        tracing::warn!(user = me.id, "Google pull after choosing calendars: {e}");
    }
    Ok(Json(current(&state, me.id).await?))
}

#[derive(Deserialize)]
struct GmailBody {
    enabled: bool,
    project_id: Option<i64>,
}

async fn set_gmail(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<GmailBody>,
) -> AppResult<Json<Status>> {
    if body.enabled {
        let project = body
            .project_id
            .ok_or_else(|| AppError::bad("choose the project emails go to"))?;
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM projects WHERE id = ? AND archived = 0)",
        )
        .bind(project)
        .fetch_one(&state.db)
        .await?;
        if !ok {
            return Err(AppError::bad("that project doesn't exist"));
        }
    }
    if body.enabled && !granted(&state.db, me.id, Feature::Gmail).await? {
        return Err(AppError::Conflict("Allow Gmail access first.".into()));
    }
    gmail::configure(&state, me.id, body.enabled, body.project_id).await?;
    Ok(Json(current(&state, me.id).await?))
}

async fn sync_now(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> AppResult<Json<Value>> {
    let report = sync::pull(&state, me.id).await?;
    Ok(Json(
        json!({ "report": report, "status": current(&state, me.id).await? }),
    ))
}

async fn push_count(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({ "pending": sync::pending(&state.db, me.id).await? }),
    ))
}

async fn push(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    // Links in the events point back at the address this browser uses for the app.
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .filter(|h| {
            !h.is_empty()
                && h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".:-[]".contains(c))
        })
        .unwrap_or("localhost:8080");
    let report = sync::push(&state, me.id, &format!("http://{host}")).await?;
    Ok(Json(
        json!({ "report": report, "status": current(&state, me.id).await? }),
    ))
}
