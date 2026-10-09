//! Shared lists (groceries, household): what we used Keep for. Built for a phone in one
//! hand: add fast, tap to check, clear the checked ones, and see the other phone's changes live.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqliteConnection;

use super::{List, Placement};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    validate,
};

const MAX_TEXT: usize = 200;
/// Pasting a whole shopping list is fine; pasting a novel is not.
const MAX_LINES: usize = 200;
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ShoppingList {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: String,
    pub created_by: Option<i64>,
    pub created_at: String,
    /// Items not yet checked off.
    pub open_count: i64,
}

const LIST_SELECT: &str = "SELECT l.id, l.name, l.color, l.position, l.created_by, l.created_at,
        (SELECT COUNT(*) FROM list_items i WHERE i.list_id = l.id AND i.checked_at IS NULL) AS open_count
    FROM lists l";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Item {
    pub id: i64,
    pub list_id: i64,
    pub text: String,
    pub note: String,
    pub position: String,
    pub checked_at: Option<String>,
    pub checked_by: Option<i64>,
    pub created_by: Option<i64>,
    pub created_at: String,
}

const ITEM_COLS: &str =
    "id, list_id, text, note, position, checked_at, checked_by, created_by, created_at";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/lists", get(all).post(create))
        .route("/lists/{id}", patch(update).delete(remove))
        .route("/lists/{id}/items", get(items).post(add_items))
        .route("/lists/{id}/clear-checked", post(clear_checked))
        .route("/list-items/{id}", patch(update_item).delete(remove_item))
}

fn lists_order() -> List {
    List {
        table: "lists",
        scope: "1 = ?",
        scope_id: 1,
    }
}

fn items_order(list_id: i64) -> List {
    List {
        table: "list_items",
        scope: "list_id = ?",
        scope_id: list_id,
    }
}

pub async fn fetch(conn: &mut SqliteConnection, id: i64) -> AppResult<ShoppingList> {
    let sql = sqlx::AssertSqlSafe(format!("{LIST_SELECT} WHERE l.id = ?"));
    Ok(sqlx::query_as(sql).bind(id).fetch_one(conn).await?)
}

async fn fetch_item(conn: &mut SqliteConnection, id: i64) -> AppResult<Item> {
    let sql = sqlx::AssertSqlSafe(format!("SELECT {ITEM_COLS} FROM list_items WHERE id = ?"));
    Ok(sqlx::query_as(sql).bind(id).fetch_one(conn).await?)
}

/// Item changes also change the list's open count, so the sidebar badge follows.
async fn announce_list(state: &AppState, by: i64, list_id: i64) {
    let Ok(mut conn) = state.db.acquire().await else {
        return;
    };
    if let Ok(list) = fetch(&mut conn, list_id).await {
        state.events.send("list.updated", by, list);
    }
}

// ---- lists ----------------------------------------------------------------------------------

async fn all(State(state): State<AppState>, _: CurrentUser) -> AppResult<Json<Vec<ShoppingList>>> {
    let sql = sqlx::AssertSqlSafe(format!("{LIST_SELECT} ORDER BY l.position, l.id"));
    Ok(Json(sqlx::query_as(sql).fetch_all(&state.db).await?))
}

#[derive(Deserialize)]
struct ListBody {
    name: Option<String>,
    color: Option<String>,
    #[serde(flatten)]
    placement: Placement,
}

pub async fn insert_list(
    conn: &mut SqliteConnection,
    name: &str,
    color: &str,
    by: i64,
) -> AppResult<ShoppingList> {
    let position = lists_order().key(conn, Placement::default(), None).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO lists (name, color, position, created_by) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(color)
    .bind(position)
    .bind(by)
    .fetch_one(&mut *conn)
    .await?;
    fetch(conn, id).await
}

async fn create(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<ListBody>,
) -> AppResult<(StatusCode, Json<ShoppingList>)> {
    let name = validate::name("name", body.name.as_deref().unwrap_or(""), 60)?;
    let color = validate::color(body.color.as_deref().unwrap_or("green"))?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let list = insert_list(&mut tx, &name, &color, me.id).await?;
    tx.commit().await?;
    state.events.send("list.created", me.id, &list);
    Ok((StatusCode::CREATED, Json(list)))
}

async fn update(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<ListBody>,
) -> AppResult<Json<ShoppingList>> {
    let name = body
        .name
        .map(|n| validate::name("name", &n, 60))
        .transpose()?;
    let color = body.color.map(|c| validate::color(&c)).transpose()?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    fetch(&mut tx, id).await?;
    let position = if body.placement.is_set() {
        Some(lists_order().key(&mut tx, body.placement, Some(id)).await?)
    } else {
        None
    };
    sqlx::query(
        "UPDATE lists SET name = COALESCE(?, name), color = COALESCE(?, color),
             position = COALESCE(?, position) WHERE id = ?",
    )
    .bind(name)
    .bind(color)
    .bind(position)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let list = fetch(&mut tx, id).await?;
    tx.commit().await?;
    state.events.send("list.updated", me.id, &list);
    Ok(Json(list))
}

async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let done = sqlx::query("DELETE FROM lists WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    state
        .events
        .send("list.deleted", me.id, json!({ "id": id }));
    Ok(StatusCode::NO_CONTENT)
}

// ---- items ----------------------------------------------------------------------------------

async fn items(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(list_id): Path<i64>,
) -> AppResult<Json<Vec<Item>>> {
    let mut conn = state.db.acquire().await?;
    fetch(&mut conn, list_id).await?;
    let sql = sqlx::AssertSqlSafe(format!(
        "SELECT {ITEM_COLS} FROM list_items WHERE list_id = ? ORDER BY position, id"
    ));
    Ok(Json(
        sqlx::query_as(sql)
            .bind(list_id)
            .fetch_all(&mut *conn)
            .await?,
    ))
}

#[derive(Deserialize)]
struct AddBody {
    /// One item per line, so a pasted shopping list becomes several items.
    text: String,
}

/// Adds lines to a list. A line matching an item that's already there (ignoring case) brings
/// that item back unchecked instead of adding a duplicate: "milk" every week reuses one item.
/// Returns the items added or brought back.
pub async fn add_lines(
    conn: &mut SqliteConnection,
    list_id: i64,
    lines: &[(String, bool)],
    by: i64,
) -> AppResult<Vec<Item>> {
    let mut out: Vec<Item> = vec![];
    for (text, checked) in lines {
        let existing: Option<(i64, Option<String>)> = sqlx::query_as(
            "SELECT id, checked_at FROM list_items WHERE list_id = ? AND text = ? COLLATE NOCASE
             ORDER BY checked_at IS NOT NULL LIMIT 1",
        )
        .bind(list_id)
        .bind(text)
        .fetch_optional(&mut *conn)
        .await?;
        let id = match existing {
            Some((id, checked_at)) => {
                if checked_at.is_some() && !checked {
                    sqlx::query(
                        "UPDATE list_items SET checked_at = NULL, checked_by = NULL WHERE id = ?",
                    )
                    .bind(id)
                    .execute(&mut *conn)
                    .await?;
                }
                id
            }
            None => {
                let position = items_order(list_id)
                    .key(conn, Placement::default(), None)
                    .await?;
                let sql = sqlx::AssertSqlSafe(format!(
                    "INSERT INTO list_items (list_id, text, position, created_by, checked_at, checked_by)
                     VALUES (?, ?, ?, ?, CASE WHEN ? THEN {NOW} END, CASE WHEN ? THEN ? END)
                     RETURNING id"
                ));
                sqlx::query_scalar(sql)
                    .bind(list_id)
                    .bind(text)
                    .bind(position)
                    .bind(by)
                    .bind(checked)
                    .bind(checked)
                    .bind(by)
                    .fetch_one(&mut *conn)
                    .await?
            }
        };
        if !out.iter().any(|i| i.id == id) {
            out.push(fetch_item(conn, id).await?);
        }
    }
    Ok(out)
}

/// Splits pasted text into item texts: one per line, trimmed, with list markers ("- ", "* ",
/// "☐ ") removed and blank lines skipped.
fn lines(text: &str) -> AppResult<Vec<(String, bool)>> {
    let out: Vec<(String, bool)> = text
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches(['-', '*', '•', '☐', '□'])
                .trim()
                .to_string()
        })
        .filter(|l| !l.is_empty())
        .map(|l| (l, false))
        .collect();
    if out.is_empty() {
        return Err(AppError::bad("type something to add"));
    }
    if out.len() > MAX_LINES {
        return Err(AppError::bad(format!("that's more than {MAX_LINES} lines")));
    }
    for (l, _) in &out {
        validate::name("item", l, MAX_TEXT)?;
    }
    Ok(out)
}

async fn add_items(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(list_id): Path<i64>,
    Json(body): Json<AddBody>,
) -> AppResult<(StatusCode, Json<Vec<Item>>)> {
    let lines = lines(&body.text)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    fetch(&mut tx, list_id).await?;
    let added = add_lines(&mut tx, list_id, &lines, me.id).await?;
    tx.commit().await?;
    for item in &added {
        state.events.send("list_item.updated", me.id, item);
    }
    announce_list(&state, me.id, list_id).await;
    Ok((StatusCode::CREATED, Json(added)))
}

#[derive(Deserialize)]
struct ItemBody {
    text: Option<String>,
    note: Option<String>,
    checked: Option<bool>,
    #[serde(flatten)]
    placement: Placement,
}

async fn update_item(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<ItemBody>,
) -> AppResult<Json<Item>> {
    let text = body
        .text
        .map(|t| validate::name("item", &t, MAX_TEXT))
        .transpose()?;
    let note = body
        .note
        .map(|n| n.trim().chars().take(500).collect::<String>());
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let item = fetch_item(&mut tx, id).await?;
    let position = if body.placement.is_set() {
        Some(
            items_order(item.list_id)
                .key(&mut tx, body.placement, Some(id))
                .await?,
        )
    } else {
        None
    };
    let sql = sqlx::AssertSqlSafe(format!(
        "UPDATE list_items SET text = COALESCE(?, text), note = COALESCE(?, note),
             position = COALESCE(?, position),
             checked_at = CASE ? WHEN 1 THEN COALESCE(checked_at, {NOW}) WHEN 0 THEN NULL ELSE checked_at END,
             checked_by = CASE ? WHEN 1 THEN COALESCE(checked_by, ?) WHEN 0 THEN NULL ELSE checked_by END
         WHERE id = ?"
    ));
    sqlx::query(sql)
        .bind(text)
        .bind(note)
        .bind(position)
        .bind(body.checked)
        .bind(body.checked)
        .bind(me.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let item = fetch_item(&mut tx, id).await?;
    tx.commit().await?;
    state.events.send("list_item.updated", me.id, &item);
    if body.checked.is_some() {
        announce_list(&state, me.id, item.list_id).await;
    }
    Ok(Json(item))
}

async fn remove_item(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let list_id: i64 = sqlx::query_scalar("DELETE FROM list_items WHERE id = ? RETURNING list_id")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    state.events.send(
        "list_item.deleted",
        me.id,
        json!({ "id": id, "list_id": list_id }),
    );
    announce_list(&state, me.id, list_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn clear_checked(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(list_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let mut conn = state.db.acquire().await?;
    fetch(&mut conn, list_id).await?;
    let done = sqlx::query("DELETE FROM list_items WHERE list_id = ? AND checked_at IS NOT NULL")
        .bind(list_id)
        .execute(&mut *conn)
        .await?;
    drop(conn);
    state
        .events
        .send("list.cleared", me.id, json!({ "id": list_id }));
    announce_list(&state, me.id, list_id).await;
    Ok(Json(json!({ "removed": done.rows_affected() })))
}

#[cfg(test)]
mod tests {
    use super::lines;

    #[test]
    fn pasted_text_becomes_items() {
        let got = lines("Milk\n\n- eggs \n* bread\n☐ coffee beans\n   ").unwrap();
        let texts: Vec<&str> = got.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(texts, ["Milk", "eggs", "bread", "coffee beans"]);
        assert!(lines(" \n ").is_err());
    }
}
