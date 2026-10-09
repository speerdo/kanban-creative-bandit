use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use super::{List, Placement};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    validate,
};

/// Columns every new project starts with (name, color, category). Editable afterwards.
const DEFAULT_STATUSES: &[(&str, &str, &str)] = &[
    ("Backlog", "slate", "todo"),
    ("To Do", "blue", "todo"),
    ("In Progress", "amber", "in_progress"),
    ("Review", "violet", "in_progress"),
    ("Done", "green", "done"),
];

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub description: String,
    pub archived: bool,
    pub position: String,
    pub created_by: Option<i64>,
    pub created_at: String,
}

const COLS: &str = "id, name, color, description, archived, position, created_by, created_at";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list).post(create))
        .route("/projects/{id}", get(get_one).patch(update).delete(remove))
}

/// Projects are one global list, ordered among the non-archived ones.
fn order() -> List {
    List {
        table: "projects",
        scope: "archived = ?",
        scope_id: 0,
    }
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default)]
    archived: bool,
}

async fn list(
    State(state): State<AppState>,
    _: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Project>>> {
    let sql = sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM projects WHERE archived = ? ORDER BY position, id"
    ));
    Ok(Json(
        sqlx::query_as(sql)
            .bind(q.archived)
            .fetch_all(&state.db)
            .await?,
    ))
}

pub async fn fetch(conn: &mut SqliteConnection, id: i64) -> AppResult<Project> {
    let sql = sqlx::AssertSqlSafe(format!("SELECT {COLS} FROM projects WHERE id = ?"));
    Ok(sqlx::query_as(sql).bind(id).fetch_one(conn).await?)
}

async fn get_one(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Project>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(fetch(&mut conn, id).await?))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    color: Option<String>,
    #[serde(default)]
    description: String,
}

async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<CreateBody>,
) -> AppResult<(StatusCode, Json<Project>)> {
    let name = validate::name("name", &body.name, 80)?;
    let color = validate::color(body.color.as_deref().unwrap_or("blue"))?;

    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let position = order().key(&mut tx, Placement::default(), None).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects (name, color, description, position, created_by)
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(color)
    .bind(body.description.trim())
    .bind(position)
    .bind(me.id)
    .fetch_one(&mut *tx)
    .await?;

    let mut pos = None;
    for (name, color, category) in DEFAULT_STATUSES {
        let p = crate::position::after(pos.as_deref());
        sqlx::query(
            "INSERT INTO statuses (project_id, name, color, category, position) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(color)
        .bind(category)
        .bind(&p)
        .execute(&mut *tx)
        .await?;
        pos = Some(p);
    }

    let project = fetch(&mut tx, id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(project)))
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
    color: Option<String>,
    description: Option<String>,
    archived: Option<bool>,
    #[serde(flatten)]
    placement: Placement,
}

async fn update(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> AppResult<Json<Project>> {
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let mut p = fetch(&mut tx, id).await?;

    if let Some(name) = body.name {
        p.name = validate::name("name", &name, 80)?;
    }
    if let Some(color) = body.color {
        p.color = validate::color(&color)?;
    }
    if let Some(description) = body.description {
        p.description = description.trim().to_string();
    }
    if let Some(archived) = body.archived
        && archived != p.archived
    {
        p.archived = archived;
        // Archiving or restoring puts the project at the end of the list it lands in.
        let list = List {
            scope_id: archived.into(),
            ..order()
        };
        p.position = list.key(&mut tx, Placement::default(), Some(id)).await?;
    }
    if body.placement.is_set() {
        let list = List {
            scope_id: p.archived.into(),
            ..order()
        };
        p.position = list.key(&mut tx, body.placement, Some(id)).await?;
    }

    sqlx::query(
        "UPDATE projects SET name = ?, color = ?, description = ?, archived = ?, position = ?
         WHERE id = ?",
    )
    .bind(&p.name)
    .bind(&p.color)
    .bind(&p.description)
    .bind(p.archived)
    .bind(&p.position)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(p))
}

/// Deletes the project with all its statuses and tasks. The UI asks first and suggests
/// archiving instead.
async fn remove(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    // Tasks first: they hold RESTRICT references to the statuses the project cascade removes.
    sqlx::query("DELETE FROM tasks WHERE project_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let done = sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
