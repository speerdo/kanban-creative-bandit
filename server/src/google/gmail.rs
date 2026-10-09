//! Gmail → tasks: label a thread `Kanban` (in any Gmail app, phone included) and the next pull
//! turns it into a task in the chosen project, then swaps the label for `Kanban/Imported`.
//! Needs `gmail.modify`, asked for only when someone turns this on.

use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use super::{Feature, GResult, Google, GoogleError, granted};
use crate::{
    AppState,
    routes::{links, tasks},
};

pub const LABEL: &str = "Kanban";
pub const IMPORTED_LABEL: &str = "Kanban/Imported";
/// Per pull; anything beyond waits for the next one.
const BATCH: usize = 25;

#[derive(Debug, Deserialize)]
struct Label {
    id: String,
    name: String,
}

#[derive(Debug)]
pub struct ThreadSummary {
    pub subject: String,
    pub from: String,
    pub snippet: String,
}

impl Google {
    /// The ids of the `Kanban` and `Kanban/Imported` labels, creating them if needed.
    pub async fn ensure_labels(&self, db: &SqlitePool, user: i64) -> GResult<(String, String)> {
        let url = self.gmail_url(&["users", "me", "labels"])?;
        let v = self
            .call(db, user, Method::GET, url.clone(), &[], None)
            .await?
            .unwrap_or_default();
        let labels: Vec<Label> = serde_json::from_value(v["labels"].clone()).unwrap_or_default();
        let find_or_create = async |name: &str| -> GResult<String> {
            if let Some(l) = labels.iter().find(|l| l.name.eq_ignore_ascii_case(name)) {
                return Ok(l.id.clone());
            }
            let body = json!({
                "name": name,
                "labelListVisibility": "labelShow",
                "messageListVisibility": "show",
            });
            let created = self
                .call(db, user, Method::POST, url.clone(), &[], Some(&body))
                .await?
                .unwrap_or_default();
            created["id"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| anyhow::anyhow!("Gmail created a label without an id").into())
        };
        let kanban = find_or_create(LABEL).await?;
        let imported = find_or_create(IMPORTED_LABEL).await?;
        Ok((kanban, imported))
    }

    async fn threads_labelled(
        &self,
        db: &SqlitePool,
        user: i64,
        label: &str,
    ) -> GResult<Vec<String>> {
        let url = self.gmail_url(&["users", "me", "threads"])?;
        let query = [
            ("labelIds", label.to_string()),
            ("maxResults", BATCH.to_string()),
        ];
        let v = self
            .call(db, user, Method::GET, url, &query, None)
            .await?
            .unwrap_or_default();
        Ok(v["threads"]
            .as_array()
            .map(|ts| {
                ts.iter()
                    .filter_map(|t| t["id"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn thread_summary(&self, db: &SqlitePool, user: i64, id: &str) -> GResult<ThreadSummary> {
        let url = self.gmail_url(&["users", "me", "threads", id])?;
        let query = [
            ("format", "metadata".to_string()),
            ("metadataHeaders", "Subject".to_string()),
            ("metadataHeaders", "From".to_string()),
        ];
        let v = self
            .call(db, user, Method::GET, url, &query, None)
            .await?
            .unwrap_or_default();
        let first = &v["messages"][0];
        let header = |name: &str| {
            first["payload"]["headers"]
                .as_array()
                .and_then(|hs| {
                    hs.iter().find(|h| {
                        h["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(name))
                    })
                })
                .and_then(|h| h["value"].as_str())
                .unwrap_or("")
                .trim()
                .to_string()
        };
        Ok(ThreadSummary {
            subject: header("Subject"),
            from: header("From"),
            snippet: html_unescape(first["snippet"].as_str().unwrap_or("")),
        })
    }

    async fn relabel(
        &self,
        db: &SqlitePool,
        user: i64,
        thread: &str,
        add: &str,
        remove: &str,
    ) -> GResult<()> {
        let url = self.gmail_url(&["users", "me", "threads", thread, "modify"])?;
        let body = json!({ "addLabelIds": [add], "removeLabelIds": [remove] });
        self.call(db, user, Method::POST, url, &[], Some(&body))
            .await?;
        Ok(())
    }
}

/// Gmail snippets come HTML-escaped.
fn html_unescape(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn description(t: &ThreadSummary, link: &str) -> String {
    let mut d = String::new();
    if !t.from.is_empty() {
        d.push_str(&format!(
            "**From:** {}\n\n",
            t.from.replace('<', "‹").replace('>', "›")
        ));
    }
    if !t.snippet.is_empty() {
        d.push_str(&format!("> {}\n\n", t.snippet));
    }
    d.push_str(&format!("[Open in Gmail]({link})"));
    d
}

#[derive(sqlx::FromRow)]
struct Settings {
    gmail_enabled: bool,
    gmail_project_id: Option<i64>,
    email: String,
}

/// Imports `Kanban`-labelled threads for `user`. Returns how many tasks were created.
pub async fn import(state: &AppState, user: i64) -> GResult<usize> {
    let db = &state.db;
    let Some(s): Option<Settings> = sqlx::query_as(
        "SELECT gmail_enabled, gmail_project_id, email FROM google_accounts WHERE user_id = ?",
    )
    .bind(user)
    .fetch_optional(db)
    .await?
    else {
        return Ok(0);
    };
    if !s.gmail_enabled {
        return Ok(0);
    }
    if !granted(db, user, Feature::Gmail).await? {
        return Err(GoogleError::Api(
            StatusCode::FORBIDDEN,
            "Gmail access isn't granted any more; turn Gmail import on again in Settings".into(),
        ));
    }
    let project: Option<i64> =
        sqlx::query_scalar("SELECT id FROM projects WHERE id = ? AND archived = 0")
            .bind(s.gmail_project_id)
            .fetch_optional(db)
            .await?;
    let Some(project) = project else {
        return Err(GoogleError::Api(
            StatusCode::CONFLICT,
            "the project for Gmail tasks is gone or archived; pick another in Settings".into(),
        ));
    };

    let g = &state.google;
    let (kanban, imported) = g.ensure_labels(db, user).await?;
    let mut created = 0;
    for thread in g.threads_labelled(db, user, &kanban).await? {
        let done: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM gmail_imports WHERE user_id = ? AND thread_id = ?)",
        )
        .bind(user)
        .bind(&thread)
        .fetch_one(db)
        .await?;
        if !done {
            let summary = g.thread_summary(db, user, &thread).await?;
            let link = format!(
                "https://mail.google.com/mail/u/?authuser={}#all/{thread}",
                s.email
            );
            let title = if summary.subject.is_empty() {
                "(no subject)"
            } else {
                summary.subject.as_str()
            };
            let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
            let task = tasks::insert(
                &mut tx,
                project,
                title,
                &description(&summary, &link),
                Some(user),
                user,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            links::insert(
                &mut tx,
                links::NewLink {
                    task_id: task.id,
                    url: &link,
                    kind: "gmail",
                    external_id: Some(&thread),
                    title: Some(title),
                    mime_type: None,
                    by: user,
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            sqlx::query("INSERT INTO gmail_imports (user_id, thread_id, task_id) VALUES (?, ?, ?)")
                .bind(user)
                .bind(&thread)
                .bind(task.id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            tasks::announce(state, user, "task.created", &task).await;
            created += 1;
        }
        // Recorded first, so a failure here just retries the label swap next time.
        g.relabel(db, user, &thread, &imported, &kanban).await?;
    }
    if created > 0 {
        tracing::info!(user, created, "imported Gmail threads");
    }
    Ok(created)
}

/// Turns Gmail import on (making sure the labels exist) or off.
pub async fn configure(
    state: &AppState,
    user: i64,
    enabled: bool,
    project: Option<i64>,
) -> GResult<()> {
    if enabled {
        if !granted(&state.db, user, Feature::Gmail).await? {
            return Err(GoogleError::Api(
                StatusCode::FORBIDDEN,
                "allow Gmail access first".into(),
            ));
        }
        state.google.ensure_labels(&state.db, user).await?;
    }
    sqlx::query(
        "UPDATE google_accounts SET gmail_enabled = ?, gmail_project_id = ? WHERE user_id = ?",
    )
    .bind(enabled)
    .bind(project)
    .bind(user)
    .execute(&state.db)
    .await?;
    Ok(())
}
