//! The Google Calendar v3 endpoints we use, with just the fields we read.

use reqwest::Method;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::{GResult, Google, GoogleError};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEntry {
    pub id: String,
    #[serde(default)]
    pub summary: String,
    pub summary_override: Option<String>,
    pub background_color: Option<String>,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub access_role: String,
}

impl CalendarEntry {
    /// The name the person sees in Google Calendar.
    pub fn name(&self) -> &str {
        self.summary_override.as_deref().unwrap_or(&self.summary)
    }

    pub fn writable(&self) -> bool {
        matches!(self.access_role.as_str(), "owner" | "writer")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub status: Option<String>,
    pub etag: Option<String>,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub html_link: Option<String>,
    pub start: Option<When>,
    pub end: Option<When>,
    pub extended_properties: Option<ExtendedProperties>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct When {
    pub date: Option<String>,
    pub date_time: Option<String>,
}

impl When {
    /// `YYYY-MM-DD` for all-day, else the RFC 3339 timestamp.
    pub fn value(&self) -> Option<&str> {
        self.date.as_deref().or(self.date_time.as_deref())
    }

    /// The calendar day (for a timed event, as written in its own offset).
    pub fn day(&self) -> Option<&str> {
        self.value().and_then(|v| v.get(..10))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtendedProperties {
    #[serde(default)]
    pub private: std::collections::HashMap<String, String>,
}

/// The private property that marks an event as one of our tasks.
pub const TASK_PROPERTY: &str = "kanbanTaskId";

impl Event {
    pub fn cancelled(&self) -> bool {
        self.status.as_deref() == Some("cancelled")
    }

    pub fn task_id(&self) -> Option<i64> {
        self.extended_properties
            .as_ref()?
            .private
            .get(TASK_PROPERTY)?
            .parse()
            .ok()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
    next_page_token: Option<String>,
    next_sync_token: Option<String>,
}

/// An all-day event for a task.
pub fn task_event(task_id: i64, summary: &str, date: &str, link: &str) -> GResult<Value> {
    let end = next_day(date)?;
    Ok(json!({
        "summary": summary,
        "description": format!("Open in Kanban: {link}"),
        "start": { "date": date },
        "end": { "date": end },
        // Shown as free, and no reminders: these are due dates, not meetings.
        "transparency": "transparent",
        "reminders": { "useDefault": false },
        "extendedProperties": { "private": { TASK_PROPERTY: task_id.to_string() } },
    }))
}

fn next_day(date: &str) -> GResult<String> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let day =
        time::Date::parse(date, format).map_err(|e| anyhow::anyhow!("bad date {date}: {e}"))?;
    let next = day
        .next_day()
        .ok_or_else(|| anyhow::anyhow!("no day after {date}"))?;
    Ok(next
        .format(format)
        .map_err(|e| anyhow::anyhow!("formatting date: {e}"))?)
}

impl Google {
    async fn pages<T: DeserializeOwned>(
        &self,
        db: &SqlitePool,
        user: i64,
        path: &[&str],
        query: &[(&str, String)],
    ) -> GResult<(Vec<T>, Option<String>)> {
        let mut items = vec![];
        let mut page_token: Option<String> = None;
        // A runaway pager would be a bug; 50 pages is far more than two people's calendars.
        for _ in 0..50 {
            let mut q = query.to_vec();
            if let Some(t) = &page_token {
                q.push(("pageToken", t.clone()));
            }
            let url = self.calendar_url(path)?;
            let v = self
                .call(db, user, Method::GET, url, &q, None)
                .await?
                .unwrap_or(Value::Null);
            let page: Page<T> = serde_json::from_value(v)
                .map_err(|e| anyhow::anyhow!("reading Google's answer: {e}"))?;
            items.extend(page.items);
            match page.next_page_token {
                Some(t) => page_token = Some(t),
                None => return Ok((items, page.next_sync_token)),
            }
        }
        Err(anyhow::anyhow!("Google kept paging; gave up after 50 pages").into())
    }

    pub async fn calendars(&self, db: &SqlitePool, user: i64) -> GResult<Vec<CalendarEntry>> {
        let query = [("maxResults", "250".to_string())];
        let (items, _) = self
            .pages(db, user, &["users", "me", "calendarList"], &query)
            .await?;
        Ok(items)
    }

    /// Creates a secondary calendar and returns its id.
    pub async fn create_calendar(
        &self,
        db: &SqlitePool,
        user: i64,
        summary: &str,
    ) -> GResult<String> {
        let url = self.calendar_url(&["calendars"])?;
        let body = json!({ "summary": summary, "description": "Tasks from the Kanban app" });
        let v = self
            .call(db, user, Method::POST, url, &[], Some(&body))
            .await?
            .unwrap_or(Value::Null);
        v["id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("Google created a calendar without an id").into())
    }

    /// Changed events since `sync_token` (or all, without one), plus the next sync token.
    pub async fn changed_events(
        &self,
        db: &SqlitePool,
        user: i64,
        calendar: &str,
        sync_token: Option<&str>,
    ) -> GResult<(Vec<Event>, Option<String>)> {
        let mut query = vec![("maxResults", "250".to_string())];
        if let Some(t) = sync_token {
            query.push(("syncToken", t.to_string()));
        }
        self.pages(db, user, &["calendars", calendar, "events"], &query)
            .await
    }

    /// Every event (recurring ones expanded) in `[from, to)`, RFC 3339 bounds.
    pub async fn events_between(
        &self,
        db: &SqlitePool,
        user: i64,
        calendar: &str,
        from: &str,
        to: &str,
    ) -> GResult<Vec<Event>> {
        let query = [
            ("timeMin", from.to_string()),
            ("timeMax", to.to_string()),
            ("singleEvents", "true".to_string()),
            ("maxResults", "2500".to_string()),
        ];
        let (items, _) = self
            .pages(db, user, &["calendars", calendar, "events"], &query)
            .await?;
        Ok(items)
    }

    pub async fn insert_event(
        &self,
        db: &SqlitePool,
        user: i64,
        calendar: &str,
        body: &Value,
    ) -> GResult<Event> {
        let url = self.calendar_url(&["calendars", calendar, "events"])?;
        let v = self
            .call(db, user, Method::POST, url, &[], Some(body))
            .await?
            .unwrap_or(Value::Null);
        Ok(serde_json::from_value(v)
            .map_err(|e| anyhow::anyhow!("reading Google's answer: {e}"))?)
    }

    pub async fn patch_event(
        &self,
        db: &SqlitePool,
        user: i64,
        calendar: &str,
        event: &str,
        body: &Value,
    ) -> GResult<Event> {
        let url = self.calendar_url(&["calendars", calendar, "events", event])?;
        let v = self
            .call(db, user, Method::PATCH, url, &[], Some(body))
            .await?
            .unwrap_or(Value::Null);
        Ok(serde_json::from_value(v)
            .map_err(|e| anyhow::anyhow!("reading Google's answer: {e}"))?)
    }

    /// Deletes an event; one that's already gone counts as deleted.
    pub async fn delete_event(
        &self,
        db: &SqlitePool,
        user: i64,
        calendar: &str,
        event: &str,
    ) -> GResult<()> {
        let url = self.calendar_url(&["calendars", calendar, "events", event])?;
        match self.call(db, user, Method::DELETE, url, &[], None).await {
            Ok(_) | Err(GoogleError::NotFound | GoogleError::Gone) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
