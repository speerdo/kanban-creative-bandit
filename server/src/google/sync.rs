//! Moving data between the app and Google Calendar.
//!
//! - **Pull** (every 15 minutes, on "Sync now", and before every push): reads changes to the
//!   tasks calendar with a sync token, so moving a task's event in Google moves its due date and
//!   deleting the event clears it. It also re-reads the overlay calendars for the Calendar view.
//! - **Push** (only on the button): makes the tasks calendar match the tasks assigned to that
//!   person that have a due date. Each task becomes one all-day event.

use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;

use super::{
    GResult, Google, GoogleError,
    calendar::{Event, task_event},
};
use crate::{
    AppState,
    routes::{activity, tasks},
};

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')";

/// How often the background job pulls.
pub const EVERY: Duration = Duration::from_secs(15 * 60);

/// Overlay events kept: from two months back to a year ahead.
const OVERLAY_PAST_DAYS: i64 = 62;
const OVERLAY_FUTURE_DAYS: i64 = 400;

#[derive(Debug, Default, Serialize)]
pub struct PullReport {
    /// Tasks whose due date followed their event in Google.
    pub tasks_changed: usize,
    pub overlay_events: usize,
    /// Gmail threads that became tasks.
    pub emails_imported: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct PushReport {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
}

/// The background job: pulls for everyone who connected Google, every 15 minutes.
pub async fn worker(state: AppState) {
    // Let startup (and the trading bot) settle first.
    tokio::time::sleep(Duration::from_secs(60)).await;
    let mut tick = tokio::time::interval(EVERY);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let users: Vec<i64> = match sqlx::query_scalar("SELECT user_id FROM google_accounts")
            .fetch_all(&state.db)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("listing Google accounts: {e}");
                continue;
            }
        };
        for user in users {
            if let Err(e) = pull(&state, user).await {
                tracing::warn!(user, "Google pull: {e}");
            }
        }
    }
}

/// Pulls one person's calendars and records the outcome (last sync time or error).
pub async fn pull(state: &AppState, user: i64) -> GResult<PullReport> {
    let g = &state.google;
    let _one_at_a_time = g.busy.lock().await;
    let result = pull_locked(state, user).await;
    record(&state.db, user, "last_sync_at", &result).await;
    state
        .events
        .send("calendar.synced", user, json!({ "user_id": user }));
    result
}

/// Pushes one person's tasks to their tasks calendar. `app_url` is where links in event
/// descriptions point (e.g. `http://192.168.4.32:8080`).
pub async fn push(state: &AppState, user: i64, app_url: &str) -> GResult<PushReport> {
    let g = &state.google;
    let _one_at_a_time = g.busy.lock().await;
    // Pull first: an event moved in Google wins over the date we'd otherwise push back.
    let pulled = pull_locked(state, user).await;
    record(&state.db, user, "last_sync_at", &pulled).await;
    pulled?;
    let result = push_locked(state, user, app_url).await;
    record(&state.db, user, "last_push_at", &result).await;
    state
        .events
        .send("calendar.synced", user, json!({ "user_id": user }));
    result
}

/// Remembers when a sync or push last worked, or why it didn't.
async fn record<T>(db: &SqlitePool, user: i64, column: &str, result: &GResult<T>) {
    let outcome =
        match result {
            Ok(_) => sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE google_accounts SET {column} = {NOW}, last_error = NULL WHERE user_id = ?"
            )))
            .bind(user)
            .execute(db)
            .await,
            Err(e) => {
                sqlx::query("UPDATE google_accounts SET last_error = ? WHERE user_id = ?")
                    .bind(e.to_string())
                    .bind(user)
                    .execute(db)
                    .await
            }
        };
    if let Err(e) = outcome {
        tracing::warn!(user, "recording the Google sync outcome: {e}");
    }
}

// ---- pull -----------------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct Calendar {
    id: i64,
    google_calendar_id: String,
    role: String,
    sync_token: Option<String>,
}

async fn pull_locked(state: &AppState, user: i64) -> GResult<PullReport> {
    let db = &state.db;
    let connected: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM google_accounts WHERE user_id = ?)")
            .bind(user)
            .fetch_one(db)
            .await?;
    if !connected {
        return Err(GoogleError::NotConnected);
    }
    let calendars: Vec<Calendar> = sqlx::query_as(
        "SELECT id, google_calendar_id, role, sync_token FROM google_calendars WHERE user_id = ?",
    )
    .bind(user)
    .fetch_all(db)
    .await?;

    let mut report = PullReport::default();
    for cal in &calendars {
        match cal.role.as_str() {
            "tasks" => report.tasks_changed += pull_tasks_calendar(state, user, cal).await?,
            _ => report.overlay_events += pull_overlay(&state.google, db, user, cal).await?,
        }
    }
    report.emails_imported = super::gmail::import(state, user).await?;
    Ok(report)
}

/// Applies changes made in Google to our pushed events. Returns how many tasks changed.
async fn pull_tasks_calendar(state: &AppState, user: i64, cal: &Calendar) -> GResult<usize> {
    let g = &state.google;
    let db = &state.db;
    let (events, next_token) = match g
        .changed_events(db, user, &cal.google_calendar_id, cal.sync_token.as_deref())
        .await
    {
        Err(GoogleError::Gone) => {
            // The sync token expired: start over with a full read.
            g.changed_events(db, user, &cal.google_calendar_id, None)
                .await?
        }
        Err(GoogleError::NotFound) => {
            return Err(GoogleError::Api(
                reqwest::StatusCode::NOT_FOUND,
                "your tasks calendar no longer exists in Google; choose another in Settings".into(),
            ));
        }
        other => other?,
    };

    let mut changed = vec![];
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    for event in &events {
        if let Some(task_id) = apply_event_change(&mut tx, user, event).await? {
            changed.push(task_id);
        }
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE google_calendars SET sync_token = ?, synced_at = {NOW} WHERE id = ?"
    )))
    .bind(next_token)
    .bind(cal.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    for task_id in &changed {
        let Ok(mut conn) = db.acquire().await else {
            break;
        };
        if let Ok(task) = tasks::fetch(&mut conn, *task_id).await {
            drop(conn);
            tasks::announce(state, user, "task.updated", &task).await;
        }
    }
    Ok(changed.len())
}

/// One event from the tasks calendar. Returns the task id when the task's due date changed.
async fn apply_event_change(
    conn: &mut sqlx::SqliteConnection,
    user: i64,
    event: &Event,
) -> GResult<Option<i64>> {
    let mapped: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT task_id, etag FROM task_events WHERE user_id = ? AND google_event_id = ?",
    )
    .bind(user)
    .bind(&event.id)
    .fetch_optional(&mut *conn)
    .await?;
    // Events we didn't push (or already deleted ourselves) aren't ours to act on.
    let Some((task_id, etag)) = mapped else {
        return Ok(None);
    };
    let due: Option<Option<String>> = sqlx::query_scalar("SELECT due_date FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(&mut *conn)
        .await?;

    if event.cancelled() {
        sqlx::query("DELETE FROM task_events WHERE user_id = ? AND google_event_id = ?")
            .bind(user)
            .bind(&event.id)
            .execute(&mut *conn)
            .await?;
        // Deleted in Google: the task stays, without its due date.
        let Some(Some(old)) = due else {
            return Ok(None);
        };
        set_due(conn, task_id, user, Some(&old), None).await?;
        return Ok(Some(task_id));
    }
    if etag.is_some() && etag == event.etag {
        return Ok(None); // our own write coming back
    }

    let Some(day) = event.start.as_ref().and_then(|s| s.day()).map(String::from) else {
        return Ok(None);
    };
    // Remember what Google now has. A title edited in Google then shows as a change waiting,
    // and the next push puts the app's title back.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE task_events SET etag = ?, pushed_date = ?, pushed_summary = ?, synced_at = {NOW}
         WHERE user_id = ? AND google_event_id = ?"
    )))
    .bind(&event.etag)
    .bind(&day)
    .bind(event.summary.as_deref().unwrap_or(""))
    .bind(user)
    .bind(&event.id)
    .execute(&mut *conn)
    .await?;

    match due {
        Some(old) if old.as_deref() != Some(day.as_str()) => {
            set_due(conn, task_id, user, old.as_deref(), Some(&day)).await?;
            Ok(Some(task_id))
        }
        _ => Ok(None),
    }
}

async fn set_due(
    conn: &mut sqlx::SqliteConnection,
    task_id: i64,
    user: i64,
    from: Option<&str>,
    to: Option<&str>,
) -> GResult<()> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE tasks SET due_date = ?, updated_at = {NOW} WHERE id = ?"
    )))
    .bind(to)
    .bind(task_id)
    .execute(&mut *conn)
    .await?;
    activity::record(conn, task_id, user, "due_google", from, to)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

/// Replaces the cached events of one overlay calendar. Returns how many it has.
async fn pull_overlay(g: &Google, db: &SqlitePool, user: i64, cal: &Calendar) -> GResult<usize> {
    let now = time::OffsetDateTime::now_utc();
    let rfc = |t: time::OffsetDateTime| {
        t.format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| GoogleError::Other(anyhow::anyhow!("formatting a time: {e}")))
    };
    let from = rfc(now - time::Duration::days(OVERLAY_PAST_DAYS))?;
    let to = rfc(now + time::Duration::days(OVERLAY_FUTURE_DAYS))?;
    let events = match g
        .events_between(db, user, &cal.google_calendar_id, &from, &to)
        .await
    {
        // Unsubscribed in Google: show nothing rather than failing everyone's sync.
        Err(GoogleError::NotFound) => vec![],
        other => other?,
    };

    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM calendar_events WHERE calendar_id = ?")
        .bind(cal.id)
        .execute(&mut *tx)
        .await?;
    let mut kept = 0;
    for e in events.iter().filter(|e| !e.cancelled()) {
        let (Some(start), Some(end)) = (
            e.start.as_ref().and_then(|w| w.value()),
            e.end.as_ref().and_then(|w| w.value()),
        ) else {
            continue;
        };
        let all_day = e.start.as_ref().is_some_and(|w| w.date.is_some());
        sqlx::query(
            "INSERT OR REPLACE INTO calendar_events
                 (calendar_id, google_event_id, summary, start_at, end_at, all_day, html_link, location)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(cal.id)
        .bind(&e.id)
        .bind(e.summary.as_deref().unwrap_or("(busy)"))
        .bind(start)
        .bind(end)
        .bind(all_day)
        .bind(&e.html_link)
        .bind(&e.location)
        .execute(&mut *tx)
        .await?;
        kept += 1;
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE google_calendars SET synced_at = {NOW} WHERE id = ?"
    )))
    .bind(cal.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(kept)
}

// ---- push -----------------------------------------------------------------------------------

/// One change waiting to go to Google.
#[derive(Debug)]
enum Op {
    Create(Want),
    Update {
        want: Want,
        event_id: String,
    },
    /// The tasks calendar changed since this was pushed: delete there, create here.
    Move {
        want: Want,
        old_calendar: String,
        event_id: String,
    },
    Delete {
        task_id: i64,
        calendar: String,
        event_id: String,
    },
}

/// A task as it should appear in Google.
#[derive(Debug, sqlx::FromRow)]
struct Want {
    task_id: i64,
    project_id: i64,
    summary: String,
    date: String,
}

#[derive(sqlx::FromRow)]
struct Pushed {
    task_id: i64,
    google_calendar_id: String,
    google_event_id: String,
    pushed_summary: String,
    pushed_date: String,
}

/// The tasks calendar's Google id, if one is chosen.
pub async fn tasks_calendar(db: &SqlitePool, user: i64) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT google_calendar_id FROM google_calendars WHERE user_id = ? AND role = 'tasks'",
    )
    .bind(user)
    .fetch_optional(db)
    .await
}

/// How many changes the next push would send (0 when no tasks calendar is chosen).
pub async fn pending(db: &SqlitePool, user: i64) -> sqlx::Result<usize> {
    match tasks_calendar(db, user).await? {
        Some(cal) => Ok(plan(db, user, &cal).await?.len()),
        None => Ok(0),
    }
}

async fn plan(db: &SqlitePool, user: i64, calendar: &str) -> sqlx::Result<Vec<Op>> {
    // Done tasks keep their event, ticked, so the calendar still shows what was finished when.
    let wants: Vec<Want> = sqlx::query_as(
        "SELECT t.id AS task_id, t.project_id,
                CASE WHEN t.completed_at IS NOT NULL THEN '✓ ' ELSE '' END || t.title AS summary,
                t.due_date AS date
         FROM tasks t JOIN projects p ON p.id = t.project_id
         WHERE t.assignee_id = ? AND t.due_date IS NOT NULL AND p.archived = 0
         ORDER BY t.due_date, t.id",
    )
    .bind(user)
    .fetch_all(db)
    .await?;
    let mut pushed: std::collections::HashMap<i64, Pushed> = sqlx::query_as::<_, Pushed>(
        "SELECT task_id, google_calendar_id, google_event_id, pushed_summary, pushed_date
         FROM task_events WHERE user_id = ?",
    )
    .bind(user)
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|p| (p.task_id, p))
    .collect();

    let mut ops = vec![];
    for want in wants {
        match pushed.remove(&want.task_id) {
            None => ops.push(Op::Create(want)),
            Some(p) if p.google_calendar_id != calendar => ops.push(Op::Move {
                want,
                old_calendar: p.google_calendar_id,
                event_id: p.google_event_id,
            }),
            Some(p) if p.pushed_summary != want.summary || p.pushed_date != want.date => {
                ops.push(Op::Update {
                    want,
                    event_id: p.google_event_id,
                })
            }
            Some(_) => {}
        }
    }
    // Pushed before, but now deleted, without a date, reassigned or archived.
    let mut gone: Vec<Pushed> = pushed.into_values().collect();
    gone.sort_by_key(|p| p.task_id);
    ops.extend(gone.into_iter().map(|p| Op::Delete {
        task_id: p.task_id,
        calendar: p.google_calendar_id,
        event_id: p.google_event_id,
    }));
    Ok(ops)
}

async fn push_locked(state: &AppState, user: i64, app_url: &str) -> GResult<PushReport> {
    let g = &state.google;
    let db = &state.db;
    let calendar = tasks_calendar(db, user).await?.ok_or_else(|| {
        GoogleError::Api(
            reqwest::StatusCode::CONFLICT,
            "choose a calendar for your tasks first (Settings → Integrations)".into(),
        )
    })?;
    let link = |w: &Want| format!("{app_url}/#/p/{}/t/{}", w.project_id, w.task_id);

    let mut report = PushReport::default();
    for op in plan(db, user, &calendar).await? {
        match op {
            Op::Create(want) => {
                let body = task_event(want.task_id, &want.summary, &want.date, &link(&want))?;
                let event = g.insert_event(db, user, &calendar, &body).await?;
                save(db, user, &calendar, &want, &event).await?;
                report.created += 1;
            }
            Op::Move {
                want,
                old_calendar,
                event_id,
            } => {
                g.delete_event(db, user, &old_calendar, &event_id).await?;
                let body = task_event(want.task_id, &want.summary, &want.date, &link(&want))?;
                let event = g.insert_event(db, user, &calendar, &body).await?;
                save(db, user, &calendar, &want, &event).await?;
                report.created += 1;
            }
            Op::Update { want, event_id } => {
                let body = task_event(want.task_id, &want.summary, &want.date, &link(&want))?;
                let event = match g.patch_event(db, user, &calendar, &event_id, &body).await {
                    // Deleted in Google since the last pull: put it back.
                    Err(GoogleError::NotFound | GoogleError::Gone) => {
                        g.insert_event(db, user, &calendar, &body).await?
                    }
                    other => other?,
                };
                save(db, user, &calendar, &want, &event).await?;
                report.updated += 1;
            }
            Op::Delete {
                task_id,
                calendar: cal,
                event_id,
            } => {
                g.delete_event(db, user, &cal, &event_id).await?;
                sqlx::query("DELETE FROM task_events WHERE task_id = ? AND user_id = ?")
                    .bind(task_id)
                    .bind(user)
                    .execute(db)
                    .await?;
                report.deleted += 1;
            }
        }
    }
    Ok(report)
}

/// Records what we just wrote to Google, one row at a time so a failed push resumes cleanly.
async fn save(
    db: &SqlitePool,
    user: i64,
    calendar: &str,
    want: &Want,
    event: &Event,
) -> GResult<()> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO task_events
             (task_id, user_id, google_calendar_id, google_event_id, etag, pushed_summary, pushed_date)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (task_id, user_id) DO UPDATE SET
             google_calendar_id = excluded.google_calendar_id,
             google_event_id = excluded.google_event_id, etag = excluded.etag,
             pushed_summary = excluded.pushed_summary, pushed_date = excluded.pushed_date,
             synced_at = {NOW}"
    )))
    .bind(want.task_id)
    .bind(user)
    .bind(calendar)
    .bind(&event.id)
    .bind(&event.etag)
    .bind(&want.summary)
    .bind(&want.date)
    .execute(db)
    .await?;
    Ok(())
}
