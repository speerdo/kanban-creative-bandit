//! Comments on a task. Anyone can comment; only the author can edit or delete their own.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
};

const MAX_BODY: usize = 20_000;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    /// Lets clients route the live event to the right project.
    pub project_id: i64,
    pub author_id: Option<i64>,
    pub body: String,
    pub created_at: String,
    pub edited_at: Option<String>,
}

const SELECT: &str = "SELECT c.id, c.task_id, t.project_id, c.author_id, c.body, c.created_at,
        c.edited_at
    FROM comments c JOIN tasks t ON t.id = c.task_id";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks/{id}/comments", get(list).post(create))
        .route("/comments/{id}", patch(update).delete(remove))
}

async fn fetch(db: &sqlx::SqlitePool, id: i64) -> AppResult<Comment> {
    let sql = sqlx::AssertSqlSafe(format!("{SELECT} WHERE c.id = ?"));
    Ok(sqlx::query_as(sql).bind(id).fetch_one(db).await?)
}

async fn list(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(task_id): Path<i64>,
) -> AppResult<Json<Vec<Comment>>> {
    let sql = sqlx::AssertSqlSafe(format!("{SELECT} WHERE c.task_id = ? ORDER BY c.id"));
    Ok(Json(
        sqlx::query_as(sql)
            .bind(task_id)
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Deserialize)]
struct Body {
    body: String,
}

fn body_text(b: &str) -> AppResult<String> {
    let b = b.trim();
    if b.is_empty() {
        return Err(AppError::bad("a comment can't be empty"));
    }
    if b.len() > MAX_BODY {
        return Err(AppError::bad("that comment is too long"));
    }
    Ok(b.to_string())
}

async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(task_id): Path<i64>,
    Json(body): Json<Body>,
) -> AppResult<(StatusCode, Json<Comment>)> {
    let text = body_text(&body.body)?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO comments (task_id, author_id, body) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(task_id)
    .bind(me.id)
    .bind(text)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::BadRequest(_) => AppError::NotFound, // the task doesn't exist
        e => e,
    })?;
    let comment = fetch(&state.db, id).await?;
    state.events.send("comment.created", me.id, &comment);
    Ok((StatusCode::CREATED, Json(comment)))
}

async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<Body>,
) -> AppResult<Json<Comment>> {
    let text = body_text(&body.body)?;
    own(&state, id, me.id).await?;
    sqlx::query(
        "UPDATE comments SET body = ?, edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(text)
    .bind(id)
    .execute(&state.db)
    .await?;
    let comment = fetch(&state.db, id).await?;
    state.events.send("comment.updated", me.id, &comment);
    Ok(Json(comment))
}

async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let c = own(&state, id, me.id).await?;
    sqlx::query("DELETE FROM comments WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    let gone = json!({ "id": id, "task_id": c.task_id, "project_id": c.project_id });
    state.events.send("comment.deleted", me.id, gone);
    Ok(StatusCode::NO_CONTENT)
}

/// The comment, if `user` wrote it.
async fn own(state: &AppState, id: i64, user: i64) -> AppResult<Comment> {
    let c = fetch(&state.db, id).await?;
    if c.author_id != Some(user) {
        return Err(AppError::Forbidden(
            "only the author can change a comment".into(),
        ));
    }
    Ok(c)
}
