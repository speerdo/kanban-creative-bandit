//! Links on a task: Gmail threads, Google Docs/Sheets/Slides/Forms, Drive files and folders,
//! or any web address. The kind comes from the URL alone; with Drive access granted, the
//! file's real name is fetched too.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqliteConnection;

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    google::{self, Feature},
};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Link {
    pub id: i64,
    pub task_id: i64,
    pub kind: String,
    pub external_id: Option<String>,
    pub url: String,
    pub title: Option<String>,
    pub mime_type: Option<String>,
    pub added_by: Option<i64>,
    pub created_at: String,
}

const COLS: &str = "id, task_id, kind, external_id, url, title, mime_type, added_by, created_at";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks/{id}/links", get(list).post(add))
        .route("/tasks/{id}/links/{link_id}", delete(remove))
}

/// What a URL points at: `(kind, external id)`.
pub fn classify(url: &Url) -> (&'static str, Option<String>) {
    let host = url.host_str().unwrap_or("");
    let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    // The id follows a `d` segment (docs, drive/file) or `folders`.
    let after = |marker: &str| {
        segments
            .iter()
            .position(|s| *s == marker)
            .and_then(|i| segments.get(i + 1))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    };
    match host {
        "mail.google.com" => {
            // …/mail/u/0/#inbox/FMfcgz…: the thread is the last part of the fragment.
            let thread = url
                .fragment()
                .and_then(|f| f.rsplit('/').next())
                .filter(|t| t.len() >= 10 && t.chars().all(|c| c.is_ascii_alphanumeric()))
                .map(String::from);
            ("gmail", thread)
        }
        "docs.google.com" => {
            let kind = match segments.first().copied() {
                Some("document") => "doc",
                Some("spreadsheets") => "sheet",
                Some("presentation") => "slides",
                Some("forms") => "form",
                _ => "drive",
            };
            // Published forms look like /forms/d/e/<id>/viewform.
            let id = match after("d").as_deref() {
                Some("e") => after("e"),
                _ => after("d"),
            };
            (kind, id)
        }
        "drive.google.com" => {
            let id = after("d").or_else(|| after("folders")).or_else(|| {
                url.query_pairs()
                    .find(|(k, _)| k == "id")
                    .map(|(_, v)| v.into_owned())
            });
            ("drive", id)
        }
        _ => ("url", None),
    }
}

pub struct NewLink<'a> {
    pub task_id: i64,
    pub url: &'a str,
    pub kind: &'a str,
    pub external_id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub mime_type: Option<&'a str>,
    pub by: i64,
}

pub async fn insert(conn: &mut SqliteConnection, l: NewLink<'_>) -> AppResult<Link> {
    let sql = sqlx::AssertSqlSafe(format!(
        "INSERT INTO task_links (task_id, kind, external_id, url, title, mime_type, added_by)
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING {COLS}"
    ));
    Ok(sqlx::query_as(sql)
        .bind(l.task_id)
        .bind(l.kind)
        .bind(l.external_id)
        .bind(l.url)
        .bind(l.title)
        .bind(l.mime_type)
        .bind(l.by)
        .fetch_one(conn)
        .await?)
}

async fn list(
    State(state): State<AppState>,
    _: CurrentUser,
    Path(task_id): Path<i64>,
) -> AppResult<Json<Vec<Link>>> {
    let sql = sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM task_links WHERE task_id = ? ORDER BY id"
    ));
    Ok(Json(
        sqlx::query_as(sql)
            .bind(task_id)
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Deserialize)]
struct AddBody {
    url: String,
    title: Option<String>,
}

async fn add(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(task_id): Path<i64>,
    Json(body): Json<AddBody>,
) -> AppResult<(StatusCode, Json<Link>)> {
    let raw = body.url.trim();
    let url = Url::parse(raw)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .ok_or_else(|| AppError::bad("paste a full web address (https://…)"))?;
    if raw.len() > 2000 {
        return Err(AppError::bad("that address is too long"));
    }
    let (kind, external_id) = classify(&url);
    let mut title = body
        .title
        .map(|t| t.trim().chars().take(200).collect::<String>())
        .filter(|t| !t.is_empty());
    let mut mime = None;

    // The file's real name, when this person granted Drive access. Best effort: a link is
    // still worth keeping if Google can't tell us (no access to that file, offline).
    if title.is_none()
        && let Some(id) = external_id
            .as_deref()
            .filter(|_| kind != "gmail" && kind != "url")
        && google::granted(&state.db, me.id, Feature::Drive).await?
    {
        match state.google.drive_file(&state.db, me.id, id).await {
            Ok(file) => {
                title = Some(file.name);
                mime = Some(file.mime_type);
            }
            Err(e) => tracing::info!(user = me.id, "Drive title for {id}: {e}"),
        }
    }

    let mut conn = state.db.acquire().await?;
    let project_id: i64 = sqlx::query_scalar("SELECT project_id FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    let link = insert(
        &mut conn,
        NewLink {
            task_id,
            url: url.as_str(),
            kind,
            external_id: external_id.as_deref(),
            title: title.as_deref(),
            mime_type: mime.as_deref(),
            by: me.id,
        },
    )
    .await?;
    let mut data = serde_json::to_value(&link).unwrap_or_default();
    data["project_id"] = json!(project_id);
    state.events.send("link.created", me.id, data);
    Ok((StatusCode::CREATED, Json(link)))
}

async fn remove(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((task_id, link_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    let done = sqlx::query("DELETE FROM task_links WHERE id = ? AND task_id = ?")
        .bind(link_id)
        .bind(task_id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    state.events.send(
        "link.deleted",
        me.id,
        json!({ "id": link_id, "task_id": task_id }),
    );
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(url: &str) -> (&'static str, Option<String>) {
        classify(&Url::parse(url).unwrap())
    }

    #[test]
    fn google_urls_are_recognised() {
        assert_eq!(
            kind("https://docs.google.com/document/d/1AbC_def-123/edit?tab=t.0"),
            ("doc", Some("1AbC_def-123".into()))
        );
        assert_eq!(
            kind("https://docs.google.com/spreadsheets/d/xyz/edit#gid=0").0,
            "sheet"
        );
        assert_eq!(
            kind("https://docs.google.com/presentation/d/xyz/edit").0,
            "slides"
        );
        assert_eq!(
            kind("https://docs.google.com/forms/d/e/xyz/viewform"),
            ("form", Some("xyz".into()))
        );
        assert_eq!(
            kind("https://drive.google.com/file/d/FILE123/view?usp=sharing"),
            ("drive", Some("FILE123".into()))
        );
        assert_eq!(
            kind("https://drive.google.com/drive/folders/FOLDER9"),
            ("drive", Some("FOLDER9".into()))
        );
        assert_eq!(
            kind("https://drive.google.com/open?id=OPEN1"),
            ("drive", Some("OPEN1".into()))
        );
        assert_eq!(
            kind("https://mail.google.com/mail/u/0/#inbox/FMfcgzQbfLxyzABC"),
            ("gmail", Some("FMfcgzQbfLxyzABC".into()))
        );
        assert_eq!(kind("https://example.com/a"), ("url", None));
    }
}
