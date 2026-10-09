//! One-off import of Google Keep notes from a Takeout export (there's no Keep API for personal
//! accounts). The browser unzips the export and sends the notes; checklists become Lists and
//! other notes become tasks. A dry run reports what would happen without changing anything.

use axum::{Json, Router, extract::State, routing::post};
use serde::{Deserialize, Serialize};

use super::{lists, tasks};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/import/keep", post(keep))
}

const MAX_NOTES: usize = 2000;

#[derive(Deserialize)]
struct KeepItem {
    text: String,
    #[serde(default)]
    checked: bool,
}

#[derive(Deserialize)]
struct KeepNote {
    #[serde(default)]
    title: String,
    #[serde(default)]
    text: String,
    /// Present for checklist notes.
    items: Option<Vec<KeepItem>>,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    trashed: bool,
}

#[derive(Deserialize)]
struct KeepBody {
    notes: Vec<KeepNote>,
    /// Where plain notes become tasks; without it they're skipped.
    project_id: Option<i64>,
    #[serde(default)]
    include_archived: bool,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Default, Serialize)]
struct ListReport {
    name: String,
    items: usize,
    /// Added to an existing list of the same name.
    merged: bool,
}

#[derive(Default, Serialize)]
struct KeepReport {
    lists: Vec<ListReport>,
    tasks: Vec<String>,
    skipped: usize,
    dry_run: bool,
}

async fn keep(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<KeepBody>,
) -> AppResult<Json<KeepReport>> {
    if body.notes.len() > MAX_NOTES {
        return Err(AppError::bad(format!("that's more than {MAX_NOTES} notes")));
    }
    let mut report = KeepReport {
        dry_run: body.dry_run,
        ..Default::default()
    };
    let mut created_lists = vec![];
    let mut created_tasks = vec![];

    // Everything in one transaction: a dry run is the same work, rolled back.
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(p) = body.project_id {
        super::projects::fetch(&mut tx, p)
            .await
            .map_err(|_| AppError::bad("that project doesn't exist"))?;
    }
    for note in body.notes {
        if note.trashed || (note.archived && !body.include_archived) {
            report.skipped += 1;
            continue;
        }
        let title = note.title.trim();
        match note.items {
            Some(items) => {
                let lines: Vec<(String, bool)> = items
                    .into_iter()
                    .map(|i| {
                        (
                            i.text.trim().chars().take(200).collect::<String>(),
                            i.checked,
                        )
                    })
                    .filter(|(t, _)| !t.is_empty())
                    .collect();
                let name: String = if title.is_empty() { "Keep list" } else { title }
                    .chars()
                    .take(60)
                    .collect();
                let existing: Option<i64> = sqlx::query_scalar(
                    "SELECT id FROM lists WHERE name = ? COLLATE NOCASE LIMIT 1",
                )
                .bind(&name)
                .fetch_optional(&mut *tx)
                .await?;
                let list_id = match existing {
                    Some(id) => id,
                    None => {
                        let list = lists::insert_list(&mut tx, &name, "green", me.id).await?;
                        created_lists.push(list.id);
                        list.id
                    }
                };
                lists::add_lines(&mut tx, list_id, &lines, me.id).await?;
                report.lists.push(ListReport {
                    name,
                    items: lines.len(),
                    merged: existing.is_some(),
                });
            }
            None => {
                let Some(project) = body.project_id else {
                    report.skipped += 1;
                    continue;
                };
                let text = note.text.trim();
                if title.is_empty() && text.is_empty() {
                    report.skipped += 1;
                    continue;
                }
                // An untitled note takes its first line as the title.
                let (title, description) = if title.is_empty() {
                    match text.split_once('\n') {
                        Some((first, rest)) => (first.trim(), rest.trim()),
                        None => (text, ""),
                    }
                } else {
                    (title, text)
                };
                let task = tasks::insert(&mut tx, project, title, description, None, me.id).await?;
                report.tasks.push(task.title.clone());
                created_tasks.push(task);
            }
        }
    }

    if body.dry_run {
        tx.rollback().await?;
        return Ok(Json(report));
    }
    tx.commit().await?;
    // A bulk change: tell open pages to refetch rather than sending hundreds of events.
    let mut conn = state.db.acquire().await?;
    for id in created_lists {
        if let Ok(list) = lists::fetch(&mut conn, id).await {
            state.events.send("list.created", me.id, list);
        }
    }
    drop(conn);
    if !report.lists.is_empty() {
        state
            .events
            .send("lists.changed", me.id, serde_json::json!({}));
    }
    for task in &created_tasks {
        state.events.send("task.created", me.id, task);
    }
    Ok(Json(report))
}
