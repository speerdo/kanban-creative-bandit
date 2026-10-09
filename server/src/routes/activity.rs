//! The per-task history shown in the detail panel ("Kat moved this to Review").

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::Serialize;
use sqlx::SqliteConnection;

use crate::{AppState, auth::CurrentUser, error::AppResult};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Activity {
    pub id: i64,
    pub task_id: i64,
    pub actor_id: Option<i64>,
    pub kind: String,
    pub from_value: Option<String>,
    pub to_value: Option<String>,
    pub created_at: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/tasks/{id}/activity", get(list))
}

async fn list(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(task_id): Path<i64>,
) -> AppResult<Json<Vec<Activity>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT id, task_id, actor_id, kind, from_value, to_value, created_at
             FROM activity WHERE task_id = ? ORDER BY id",
        )
        .bind(task_id)
        .fetch_all(&state.db)
        .await?,
    ))
}

/// Appends one entry. `kind` is e.g. `created`, `status`, `assignee`, `due`, `priority`, `title`,
/// `description`, `labels`, `completed`, `reopened`.
pub async fn record(
    conn: &mut SqliteConnection,
    task_id: i64,
    actor: i64,
    kind: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO activity (task_id, actor_id, kind, from_value, to_value) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(task_id)
    .bind(actor)
    .bind(kind)
    .bind(from)
    .bind(to)
    .execute(conn)
    .await?;
    Ok(())
}

/// Display names for history entries.
pub async fn user_name(conn: &mut SqliteConnection, id: Option<i64>) -> AppResult<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    Ok(
        sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(conn)
            .await?,
    )
}

pub async fn label_names(conn: &mut SqliteConnection, task_id: i64) -> AppResult<String> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT l.name FROM task_labels tl JOIN labels l ON l.id = tl.label_id
         WHERE tl.task_id = ? ORDER BY l.name COLLATE NOCASE",
    )
    .bind(task_id)
    .fetch_all(conn)
    .await?;
    Ok(names.join(", "))
}
