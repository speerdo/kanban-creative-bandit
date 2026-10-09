//! Labels are global: one shared set (e.g. "bug", "garden") usable in every project.

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
    validate,
};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Label {
    pub id: i64,
    pub name: String,
    pub color: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/labels", get(list).post(create))
        .route("/labels/{id}", patch(update).delete(remove))
}

async fn list(State(state): State<AppState>, _: CurrentUser) -> AppResult<Json<Vec<Label>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, name, color FROM labels ORDER BY name COLLATE NOCASE")
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Deserialize)]
struct Body {
    name: Option<String>,
    color: Option<String>,
}

fn duplicate(e: sqlx::Error) -> AppError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            AppError::Conflict("a label with that name already exists".into())
        }
        _ => e.into(),
    }
}

async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<Body>,
) -> AppResult<(StatusCode, Json<Label>)> {
    let name = validate::name("name", body.name.as_deref().unwrap_or(""), 40)?;
    let color = validate::color(body.color.as_deref().unwrap_or("slate"))?;
    let label: Label =
        sqlx::query_as("INSERT INTO labels (name, color) VALUES (?, ?) RETURNING id, name, color")
            .bind(name)
            .bind(color)
            .fetch_one(&state.db)
            .await
            .map_err(duplicate)?;
    state.events.send("label.created", me.id, &label);
    Ok((StatusCode::CREATED, Json(label)))
}

async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<Body>,
) -> AppResult<Json<Label>> {
    let name = body
        .name
        .map(|n| validate::name("name", &n, 40))
        .transpose()?;
    let color = body.color.map(|c| validate::color(&c)).transpose()?;
    let label: Label = sqlx::query_as(
        "UPDATE labels SET name = COALESCE(?, name), color = COALESCE(?, color) WHERE id = ?
         RETURNING id, name, color",
    )
    .bind(name)
    .bind(color)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(duplicate)?
    .ok_or(AppError::NotFound)?;
    state.events.send("label.updated", me.id, &label);
    Ok(Json(label))
}

async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let done = sqlx::query("DELETE FROM labels WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    state
        .events
        .send("label.deleted", me.id, json!({ "id": id }));
    Ok(StatusCode::NO_CONTENT)
}
