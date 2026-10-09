use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use super::{List, Placement, projects};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    validate,
};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Status {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub color: String,
    pub category: String,
    pub position: String,
}

const COLS: &str = "id, project_id, name, color, category, position";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{id}/statuses", get(list).post(create))
        .route("/statuses/{id}", patch(update).delete(remove))
}

fn order(project_id: i64) -> List {
    List {
        table: "statuses",
        scope: "project_id = ?",
        scope_id: project_id,
    }
}

pub async fn fetch(conn: &mut SqliteConnection, id: i64) -> AppResult<Status> {
    let sql = sqlx::AssertSqlSafe(format!("SELECT {COLS} FROM statuses WHERE id = ?"));
    Ok(sqlx::query_as(sql).bind(id).fetch_one(conn).await?)
}

async fn list(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(project_id): Path<i64>,
) -> AppResult<Json<Vec<Status>>> {
    let mut conn = state.db.acquire().await?;
    projects::fetch(&mut conn, project_id).await?;
    let sql = sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM statuses WHERE project_id = ? ORDER BY position, id"
    ));
    Ok(Json(
        sqlx::query_as(sql)
            .bind(project_id)
            .fetch_all(&mut *conn)
            .await?,
    ))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    color: Option<String>,
    category: Option<String>,
    #[serde(flatten)]
    placement: Placement,
}

async fn create(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(project_id): Path<i64>,
    Json(body): Json<CreateBody>,
) -> AppResult<(StatusCode, Json<Status>)> {
    let name = validate::name("name", &body.name, 40)?;
    let color = validate::color(body.color.as_deref().unwrap_or("slate"))?;
    let category = validate::one_of(
        "category",
        body.category.as_deref().unwrap_or("todo"),
        validate::CATEGORIES,
    )?;

    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    projects::fetch(&mut tx, project_id).await?;
    let position = order(project_id).key(&mut tx, body.placement, None).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO statuses (project_id, name, color, category, position)
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(project_id)
    .bind(name)
    .bind(color)
    .bind(category)
    .bind(position)
    .fetch_one(&mut *tx)
    .await?;
    let status = fetch(&mut tx, id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(status)))
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
    color: Option<String>,
    category: Option<String>,
    #[serde(flatten)]
    placement: Placement,
}

async fn update(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> AppResult<Json<Status>> {
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let mut s = fetch(&mut tx, id).await?;

    if let Some(name) = body.name {
        s.name = validate::name("name", &name, 40)?;
    }
    if let Some(color) = body.color {
        s.color = validate::color(&color)?;
    }
    if let Some(category) = body.category {
        let category = validate::one_of("category", &category, validate::CATEGORIES)?;
        if category != s.category {
            // Turning a column into (or out of) a "done" column completes (or reopens) its tasks.
            sqlx::query(
                "UPDATE tasks SET completed_at = CASE WHEN ? = 'done'
                     THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ELSE NULL END
                 WHERE status_id = ?",
            )
            .bind(&category)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            s.category = category;
        }
    }
    if body.placement.is_set() {
        s.position = order(s.project_id)
            .key(&mut tx, body.placement, Some(id))
            .await?;
    }

    sqlx::query("UPDATE statuses SET name = ?, color = ?, category = ?, position = ? WHERE id = ?")
        .bind(&s.name)
        .bind(&s.color)
        .bind(&s.category)
        .bind(&s.position)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(s))
}

#[derive(Deserialize)]
struct RemoveQuery {
    /// Where the column's tasks go. Required when it still has tasks.
    move_to: Option<i64>,
}

async fn remove(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
    Query(q): Query<RemoveQuery>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let s = fetch(&mut tx, id).await?;

    let siblings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM statuses WHERE project_id = ?")
        .bind(s.project_id)
        .fetch_one(&mut *tx)
        .await?;
    if siblings <= 1 {
        return Err(AppError::Conflict(
            "a project needs at least one status".into(),
        ));
    }

    let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE status_id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if tasks > 0 {
        let Some(target) = q.move_to.filter(|&t| t != id) else {
            return Err(AppError::Conflict(format!(
                "\"{}\" still has {tasks} task(s); choose a status to move them to",
                s.name
            )));
        };
        let target = fetch(&mut tx, target).await?;
        if target.project_id != s.project_id {
            return Err(AppError::bad(
                "move_to must be a status in the same project",
            ));
        }
        // Moved tasks go after the target's existing ones, keeping their relative order.
        let last: Option<String> =
            sqlx::query_scalar("SELECT MAX(position) FROM tasks WHERE status_id = ?")
                .bind(target.id)
                .fetch_one(&mut *tx)
                .await?;
        let moving: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM tasks WHERE status_id = ? ORDER BY position, id")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        let mut pos = last;
        for task_id in moving {
            let p = crate::position::after(pos.as_deref());
            sqlx::query(
                "UPDATE tasks SET status_id = ?, position = ?,
                     completed_at = CASE WHEN ? = 'done'
                         THEN COALESCE(completed_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                         ELSE NULL END,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                 WHERE id = ?",
            )
            .bind(target.id)
            .bind(&p)
            .bind(&target.category)
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
            pos = Some(p);
        }
    }

    sqlx::query("DELETE FROM statuses WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
