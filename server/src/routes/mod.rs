//! JSON API for the task data. Every handler here takes `CurrentUser`, so all of it
//! requires a session.

pub mod activity;
mod calendar;
mod comments;
mod import;
mod labels;
pub mod links;
mod lists;
mod projects;
mod statuses;
pub mod tasks;

use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;
use sqlx::{AssertSqlSafe, SqliteConnection};

use crate::{
    AppState,
    auth::{CurrentUser, User},
    error::{AppError, AppResult},
    position,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .merge(activity::router())
        .merge(calendar::router())
        .merge(comments::router())
        .merge(labels::router())
        .merge(links::router())
        .merge(lists::router())
        .merge(import::router())
        .merge(projects::router())
        .merge(statuses::router())
        .merge(tasks::router())
}

/// Everyone who can be assigned a task.
async fn list_users(State(state): State<AppState>, _: CurrentUser) -> AppResult<Json<Vec<User>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT id, username, display_name, avatar_color FROM users ORDER BY display_name",
        )
        .fetch_all(&state.db)
        .await?,
    ))
}

/// Where to put an item in its list: right after one item, right before one, or at the end
/// when neither is given.
#[derive(Debug, Default, Deserialize, Clone, Copy)]
pub struct Placement {
    pub after_id: Option<i64>,
    pub before_id: Option<i64>,
}

impl Placement {
    pub fn is_set(&self) -> bool {
        self.after_id.is_some() || self.before_id.is_some()
    }
}

/// One ordered list in one table: `table` rows matching `scope` (an SQL condition with a single
/// `?` bound to `scope_id`), sorted by `position`. Table and scope are always literals from
/// this crate, never user input.
pub struct List {
    pub table: &'static str,
    pub scope: &'static str,
    pub scope_id: i64,
}

impl List {
    /// A position key for `placement`, ignoring the row being moved (`moving`).
    pub async fn key(
        &self,
        conn: &mut SqliteConnection,
        placement: Placement,
        moving: Option<i64>,
    ) -> AppResult<String> {
        let moving = moving.unwrap_or(-1);
        if placement.after_id == Some(moving) || placement.before_id == Some(moving) {
            return Err(AppError::bad("can't place an item next to itself"));
        }
        let (a, b) = match (placement.after_id, placement.before_id) {
            (Some(a), _) => {
                let a = self.position_of(conn, a).await?;
                let b = self.neighbour(conn, &a, ">", "MIN", moving).await?;
                (Some(a), b)
            }
            (None, Some(b)) => {
                let b = self.position_of(conn, b).await?;
                let a = self.neighbour(conn, &b, "<", "MAX", moving).await?;
                (a, Some(b))
            }
            (None, None) => {
                let sql = format!(
                    "SELECT MAX(position) FROM {} WHERE {} AND id != ?",
                    self.table, self.scope
                );
                let last: Option<String> = sqlx::query_scalar(AssertSqlSafe(sql))
                    .bind(self.scope_id)
                    .bind(moving)
                    .fetch_one(&mut *conn)
                    .await?;
                return Ok(position::after(last.as_deref()));
            }
        };
        // Two keys can collide if both of us insert at the same spot at the same moment;
        // then there's no room "between" them, so go just after `a` instead.
        match (&a, &b) {
            (Some(a), Some(b)) if a >= b => Ok(position::between(Some(a), None)),
            _ => Ok(position::between(a.as_deref(), b.as_deref())),
        }
    }

    async fn position_of(&self, conn: &mut SqliteConnection, id: i64) -> AppResult<String> {
        let sql = format!(
            "SELECT position FROM {} WHERE id = ? AND {}",
            self.table, self.scope
        );
        sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(id)
            .bind(self.scope_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| AppError::bad("the item to place next to isn't in this list"))
    }

    async fn neighbour(
        &self,
        conn: &mut SqliteConnection,
        of: &str,
        cmp: &str,
        agg: &str,
        moving: i64,
    ) -> AppResult<Option<String>> {
        let sql = format!(
            "SELECT {agg}(position) FROM {} WHERE {} AND id != ? AND position {cmp} ?",
            self.table, self.scope
        );
        Ok(sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(self.scope_id)
            .bind(moving)
            .bind(of)
            .fetch_one(&mut *conn)
            .await?)
    }
}
