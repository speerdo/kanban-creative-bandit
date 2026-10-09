//! The Calendar view's data: tasks due in a range, plus the signed-in person's cached Google
//! overlay events.

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::tasks;
use crate::{AppState, auth::CurrentUser, error::AppResult, validate};

pub fn router() -> Router<AppState> {
    Router::new().route("/calendar", get(range))
}

#[derive(Deserialize)]
struct RangeQuery {
    /// Inclusive, `YYYY-MM-DD`.
    from: String,
    to: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct OverlayEvent {
    google_event_id: String,
    calendar: String,
    color: Option<String>,
    summary: String,
    start_at: String,
    end_at: String,
    all_day: bool,
    html_link: Option<String>,
    location: Option<String>,
}

async fn range(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Query(q): Query<RangeQuery>,
) -> AppResult<Json<Value>> {
    let from = validate::date(&q.from)?;
    let to = validate::date(&q.to)?;
    if from > to {
        return Err(crate::error::AppError::bad("from must not be after to"));
    }
    let sql = sqlx::AssertSqlSafe(format!(
        "{} JOIN projects p ON p.id = t.project_id
         WHERE p.archived = 0 AND t.due_date BETWEEN ? AND ?
         ORDER BY t.due_date, t.completed_at IS NOT NULL, t.position, t.id LIMIT 3000",
        tasks::SELECT
    ));
    let tasks: Vec<tasks::Task> = sqlx::query_as(sql)
        .bind(&from)
        .bind(&to)
        .fetch_all(&state.db)
        .await?;

    // Day prefixes are compared as text; a timed event's day is in its own offset, so the
    // browser does the exact placement.
    let events: Vec<OverlayEvent> = sqlx::query_as(
        "SELECT e.google_event_id, c.summary AS calendar, c.color, e.summary, e.start_at, e.end_at,
                e.all_day, e.html_link, e.location
         FROM calendar_events e JOIN google_calendars c ON c.id = e.calendar_id
         WHERE c.user_id = ? AND c.role = 'overlay'
           AND substr(e.start_at, 1, 10) <= date(?, '+1 day')
           AND substr(e.end_at, 1, 10) >= date(?, '-1 day')
         ORDER BY e.all_day DESC, e.start_at
         LIMIT 3000",
    )
    .bind(me.id)
    .bind(&to)
    .bind(&from)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "tasks": tasks, "events": events })))
}
