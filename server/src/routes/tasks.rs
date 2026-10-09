use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use super::{List, Placement, projects, statuses};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    validate,
};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub status_id: i64,
    pub parent_task_id: Option<i64>,
    pub title: String,
    pub description: String,
    pub assignee_id: Option<i64>,
    pub priority: String,
    pub due_date: Option<String>,
    pub start_date: Option<String>,
    pub position: String,
    pub completed_at: Option<String>,
    pub created_by: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub subtask_count: i64,
    pub subtasks_done: i64,
}

const SELECT: &str = "SELECT t.id, t.project_id, t.status_id, t.parent_task_id, t.title,
        t.description, t.assignee_id, t.priority, t.due_date, t.start_date, t.position,
        t.completed_at, t.created_by, t.created_at, t.updated_at,
        (SELECT COUNT(*) FROM tasks s WHERE s.parent_task_id = t.id) AS subtask_count,
        (SELECT COUNT(*) FROM tasks s WHERE s.parent_task_id = t.id
            AND s.completed_at IS NOT NULL) AS subtasks_done
    FROM tasks t";

const MAX_TITLE: usize = 300;
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks", get(list).post(create))
        .route("/tasks/{id}", get(get_one).patch(update).delete(remove))
        .route("/tasks/{id}/move", post(move_task))
}

/// Top-level tasks are ordered within their status; subtasks within their parent.
fn order(status_id: i64, parent: Option<i64>) -> List {
    match parent {
        Some(p) => List {
            table: "tasks",
            scope: "parent_task_id = ?",
            scope_id: p,
        },
        None => List {
            table: "tasks",
            scope: "status_id = ? AND parent_task_id IS NULL",
            scope_id: status_id,
        },
    }
}

pub async fn fetch(conn: &mut SqliteConnection, id: i64) -> AppResult<Task> {
    let sql = sqlx::AssertSqlSafe(format!("{SELECT} WHERE t.id = ?"));
    Ok(sqlx::query_as(sql).bind(id).fetch_one(conn).await?)
}

// ---- list -------------------------------------------------------------------------------

#[derive(Deserialize)]
struct ListQuery {
    project: Option<i64>,
    status: Option<i64>,
    /// A user id, `me`, or `none` (unassigned).
    assignee: Option<String>,
    /// Inclusive, `YYYY-MM-DD`.
    due_before: Option<String>,
    /// Subtasks of this task. Without it, only top-level tasks are listed.
    parent: Option<i64>,
    /// Include completed tasks (default true; My Tasks turns it off).
    completed: Option<bool>,
    /// Case-insensitive title search.
    q: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Query(f): Query<ListQuery>,
) -> AppResult<Json<Vec<Task>>> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(SELECT);
    qb.push(" JOIN projects p ON p.id = t.project_id WHERE p.archived = 0");

    match f.parent {
        Some(parent) => {
            qb.push(" AND t.parent_task_id = ").push_bind(parent);
        }
        None => {
            qb.push(" AND t.parent_task_id IS NULL");
        }
    }
    if let Some(project) = f.project {
        qb.push(" AND t.project_id = ").push_bind(project);
    }
    if let Some(status) = f.status {
        qb.push(" AND t.status_id = ").push_bind(status);
    }
    match f.assignee.as_deref() {
        None | Some("") => {}
        Some("none") => {
            qb.push(" AND t.assignee_id IS NULL");
        }
        Some("me") => {
            qb.push(" AND t.assignee_id = ").push_bind(me.id);
        }
        Some(id) => {
            let id: i64 = id
                .parse()
                .map_err(|_| AppError::bad("assignee must be a user id, 'me' or 'none'"))?;
            qb.push(" AND t.assignee_id = ").push_bind(id);
        }
    }
    if let Some(d) = f.due_before.as_deref() {
        qb.push(" AND t.due_date <= ").push_bind(validate::date(d)?);
    }
    if f.completed == Some(false) {
        qb.push(" AND t.completed_at IS NULL");
    }
    if let Some(q) = f.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let escaped = q
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        qb.push(" AND t.title LIKE ")
            .push_bind(format!("%{escaped}%"))
            .push(" ESCAPE '\\'");
    }
    qb.push(" ORDER BY t.project_id, t.position, t.id LIMIT 2000");

    Ok(Json(qb.build_query_as().fetch_all(&state.db).await?))
}

async fn get_one(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Task>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(fetch(&mut conn, id).await?))
}

// ---- create -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateBody {
    project_id: Option<i64>,
    status_id: Option<i64>,
    parent_task_id: Option<i64>,
    title: String,
    #[serde(default)]
    description: String,
    assignee_id: Option<i64>,
    priority: Option<String>,
    due_date: Option<String>,
    start_date: Option<String>,
    #[serde(flatten)]
    placement: Placement,
}

async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<CreateBody>,
) -> AppResult<(StatusCode, Json<Task>)> {
    let title = validate::name("title", &body.title, MAX_TITLE)?;
    let priority = validate::one_of(
        "priority",
        body.priority.as_deref().unwrap_or("none"),
        validate::PRIORITIES,
    )?;
    let due_date = body.due_date.as_deref().map(validate::date).transpose()?;
    let start_date = body.start_date.as_deref().map(validate::date).transpose()?;

    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;

    // A subtask lives in its parent's project; a status implies its project.
    let parent = match body.parent_task_id {
        Some(p) => Some(
            fetch(&mut tx, p)
                .await
                .map_err(not_found_as("parent task"))?,
        ),
        None => None,
    };
    let status = match body.status_id {
        Some(s) => Some(
            statuses::fetch(&mut tx, s)
                .await
                .map_err(not_found_as("status"))?,
        ),
        None => None,
    };
    let project_id = body
        .project_id
        .or(parent.as_ref().map(|p| p.project_id))
        .or(status.as_ref().map(|s| s.project_id))
        .ok_or_else(|| AppError::bad("project_id is required"))?;
    projects::fetch(&mut tx, project_id)
        .await
        .map_err(not_found_as("project"))?;
    if parent.as_ref().is_some_and(|p| p.project_id != project_id)
        || status.as_ref().is_some_and(|s| s.project_id != project_id)
    {
        return Err(AppError::bad(
            "status, parent and project must all be the same project",
        ));
    }

    let status = match status {
        Some(s) => s,
        // Subtasks start in their parent's status; new tasks in the project's first column.
        None => match &parent {
            Some(p) => statuses::fetch(&mut tx, p.status_id).await?,
            None => first_status(&mut tx, project_id, None).await?,
        },
    };
    let position = order(status.id, body.parent_task_id)
        .key(&mut tx, body.placement, None)
        .await?;

    let sql = sqlx::AssertSqlSafe(format!(
        "INSERT INTO tasks (project_id, status_id, parent_task_id, title, description, assignee_id,
             priority, due_date, start_date, position, completed_at, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CASE WHEN ? = 'done' THEN {NOW} END, ?)
         RETURNING id"
    ));
    let id: i64 = sqlx::query_scalar(sql)
        .bind(project_id)
        .bind(status.id)
        .bind(body.parent_task_id)
        .bind(title)
        .bind(body.description.trim())
        .bind(body.assignee_id)
        .bind(priority)
        .bind(due_date)
        .bind(start_date)
        .bind(position)
        .bind(&status.category)
        .bind(me.id)
        .fetch_one(&mut *tx)
        .await?;

    let task = fetch(&mut tx, id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(task)))
}

// ---- update -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct UpdateBody {
    title: Option<String>,
    description: Option<String>,
    #[serde(default, deserialize_with = "validate::nullable")]
    assignee_id: Option<Option<i64>>,
    priority: Option<String>,
    #[serde(default, deserialize_with = "validate::nullable")]
    due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "validate::nullable")]
    start_date: Option<Option<String>>,
    /// Moves the task to the end of that status.
    status_id: Option<i64>,
    /// Shortcut for the checkbox: `true` moves to the project's done column, `false` back to
    /// its last to-do column.
    completed: Option<bool>,
}

async fn update(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> AppResult<Json<Task>> {
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let mut t = fetch(&mut tx, id).await?;

    if let Some(title) = body.title {
        t.title = validate::name("title", &title, MAX_TITLE)?;
    }
    if let Some(description) = body.description {
        t.description = description.trim().to_string();
    }
    if let Some(assignee) = body.assignee_id {
        t.assignee_id = assignee;
    }
    if let Some(priority) = body.priority {
        t.priority = validate::one_of("priority", &priority, validate::PRIORITIES)?;
    }
    if let Some(due) = body.due_date {
        t.due_date = due.as_deref().map(validate::date).transpose()?;
    }
    if let Some(start) = body.start_date {
        t.start_date = start.as_deref().map(validate::date).transpose()?;
    }
    sqlx::query(
        "UPDATE tasks SET title = ?, description = ?, assignee_id = ?, priority = ?,
             due_date = ?, start_date = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&t.title)
    .bind(&t.description)
    .bind(t.assignee_id)
    .bind(&t.priority)
    .bind(&t.due_date)
    .bind(&t.start_date)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    let target = match (body.status_id, body.completed) {
        (Some(s), _) => Some(s),
        (None, Some(true)) if t.completed_at.is_none() => {
            Some(first_status(&mut tx, t.project_id, Some("done")).await?.id)
        }
        (None, Some(false)) if t.completed_at.is_some() => {
            Some(reopen_status(&mut tx, t.project_id).await?)
        }
        _ => None,
    };
    if let Some(status_id) = target.filter(|&s| s != t.status_id) {
        place(&mut tx, &t, status_id, Placement::default()).await?;
    }

    let task = fetch(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(task))
}

// ---- move -------------------------------------------------------------------------------

#[derive(Deserialize)]
struct MoveBody {
    /// Defaults to the current status (a reorder within the column).
    status_id: Option<i64>,
    #[serde(flatten)]
    placement: Placement,
}

/// The single drag-and-drop endpoint: change status and/or position in one call.
async fn move_task(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<MoveBody>,
) -> AppResult<Json<Task>> {
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let t = fetch(&mut tx, id).await?;
    place(
        &mut tx,
        &t,
        body.status_id.unwrap_or(t.status_id),
        body.placement,
    )
    .await?;
    let task = fetch(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(task))
}

/// Puts `t` into `status_id` at `placement`, updating `completed_at` from the status category.
async fn place(
    conn: &mut SqliteConnection,
    t: &Task,
    status_id: i64,
    placement: Placement,
) -> AppResult<()> {
    let status = statuses::fetch(conn, status_id)
        .await
        .map_err(not_found_as("status"))?;
    if status.project_id != t.project_id {
        return Err(AppError::bad(
            "can't move a task to another project's status",
        ));
    }
    let position = order(status.id, t.parent_task_id)
        .key(conn, placement, Some(t.id))
        .await?;
    let sql = sqlx::AssertSqlSafe(format!(
        "UPDATE tasks SET status_id = ?, position = ?,
             completed_at = CASE WHEN ? = 'done' THEN COALESCE(completed_at, {NOW}) END,
             updated_at = {NOW}
         WHERE id = ?"
    ));
    sqlx::query(sql)
        .bind(status.id)
        .bind(position)
        .bind(&status.category)
        .bind(t.id)
        .execute(conn)
        .await?;
    Ok(())
}

// ---- delete -----------------------------------------------------------------------------

async fn remove(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let done = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---- helpers ----------------------------------------------------------------------------

/// The first status of a project, optionally only of one category.
async fn first_status(
    conn: &mut SqliteConnection,
    project_id: i64,
    category: Option<&str>,
) -> AppResult<statuses::Status> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM statuses WHERE project_id = ? AND (? IS NULL OR category = ?)
         ORDER BY position, id LIMIT 1",
    )
    .bind(project_id)
    .bind(category)
    .bind(category)
    .fetch_optional(&mut *conn)
    .await?;
    let id = id.ok_or_else(|| match category {
        Some(c) => AppError::Conflict(format!("this project has no '{c}' status")),
        None => AppError::Conflict("this project has no statuses".into()),
    })?;
    statuses::fetch(conn, id).await
}

/// Unchecking a task sends it to the last to-do column ("To Do", not "Backlog").
async fn reopen_status(conn: &mut SqliteConnection, project_id: i64) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT id FROM statuses WHERE project_id = ? AND category != 'done'
         ORDER BY category = 'todo' DESC, CASE WHEN category = 'todo' THEN position END DESC,
                  position
         LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| AppError::Conflict("this project has no open status to move it to".into()))
}

/// A missing referenced row is the caller's mistake (400), not a missing resource (404).
fn not_found_as(what: &'static str) -> impl Fn(AppError) -> AppError {
    move |e| match e {
        AppError::NotFound => AppError::bad(format!("that {what} doesn't exist")),
        e => e,
    }
}
