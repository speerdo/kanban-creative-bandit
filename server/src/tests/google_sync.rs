//! Google Calendar sync against a stand-in Google: a small axum server that speaks just
//! enough OAuth and Calendar v3 (tokens, calendar list, events with sync tokens, revoke).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use base64::Engine as _;
use serde_json::{Value, json};

use super::Client;
use crate::google::{Config, Endpoints};

const PRIMARY: &str = "adam@example.com";
const FAMILY: &str = "family#holiday@group.v.calendar.google.com";
const CREATED: &str = "kanban-1@group.calendar.google.com";

#[derive(Clone, Debug)]
struct FakeEvent {
    id: String,
    summary: String,
    start: Value,
    end: Value,
    cancelled: bool,
    etag: String,
    seq: u64,
    private: Value,
}

impl FakeEvent {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "status": if self.cancelled { "cancelled" } else { "confirmed" },
            "etag": self.etag,
            "summary": self.summary,
            "start": self.start,
            "end": self.end,
            "htmlLink": format!("https://calendar.google.com/event?eid={}", self.id),
            "extendedProperties": { "private": self.private },
        })
    }
}

#[derive(Default)]
struct World {
    seq: u64,
    calendars: Vec<(String, String, bool)>,
    events: HashMap<String, Vec<FakeEvent>>,
    revoked: bool,
    refreshes: u32,
}

impl World {
    fn bump(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

#[derive(Clone)]
struct Fake(Arc<Mutex<World>>);

impl Fake {
    async fn start() -> (Self, Endpoints) {
        let fake = Fake(Arc::default());
        {
            let mut w = fake.0.lock().unwrap();
            w.calendars = vec![
                (PRIMARY.into(), "Adam".into(), true),
                (FAMILY.into(), "Holidays".into(), false),
            ];
        }
        let app = Router::new()
            .route("/token", post(token))
            .route("/revoke", post(revoke))
            .route("/cal/users/me/calendarList", get(calendar_list))
            .route("/cal/calendars", post(create_calendar))
            .route(
                "/cal/calendars/{cal}/events",
                get(list_events).post(insert_event),
            )
            .route(
                "/cal/calendars/{cal}/events/{id}",
                patch(patch_event).delete(delete_event),
            )
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (fake, Endpoints::at(&base))
    }

    fn events(&self, cal: &str) -> Vec<FakeEvent> {
        self.0
            .lock()
            .unwrap()
            .events
            .get(cal)
            .cloned()
            .unwrap_or_default()
    }

    /// Someone edits an event in Google Calendar.
    fn edit(&self, cal: &str, id: &str, f: impl FnOnce(&mut FakeEvent)) {
        let mut w = self.0.lock().unwrap();
        let seq = w.bump();
        let e = w
            .events
            .get_mut(cal)
            .unwrap()
            .iter_mut()
            .find(|e| e.id == id)
            .unwrap();
        f(e);
        e.seq = seq;
        e.etag = format!("\"etag-{seq}\"");
    }

    fn add(&self, cal: &str, summary: &str, start: Value, end: Value) {
        let mut w = self.0.lock().unwrap();
        let seq = w.bump();
        w.events.entry(cal.into()).or_default().push(FakeEvent {
            id: format!("ev{seq}"),
            summary: summary.into(),
            start,
            end,
            cancelled: false,
            etag: format!("\"etag-{seq}\""),
            seq,
            private: json!({}),
        });
    }
}

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h == "Bearer access-1")
}

fn id_token() -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let claims = json!({ "sub": "google-sub-1", "email": "adam@example.com" });
    format!(
        "{}.{}.sig",
        b64.encode("{}"),
        b64.encode(claims.to_string())
    )
}

async fn token(State(fake): State<Fake>, Form(form): Form<HashMap<String, String>>) -> Response {
    let mut w = fake.0.lock().unwrap();
    let grant = form.get("grant_type").map(String::as_str);
    let bad = |e: &str| (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response();
    if form.get("client_secret").map(String::as_str) != Some("secret") {
        return bad("invalid_client");
    }
    match grant {
        Some("authorization_code") => {
            if form.get("code").map(String::as_str) != Some("good-code")
                || form.get("code_verifier").is_none_or(|v| v.len() < 43)
                || form.get("redirect_uri").map(String::as_str) != Some("http://127.0.0.1:8642/")
            {
                return bad("invalid_grant");
            }
            w.revoked = false;
            Json(json!({
                "access_token": "access-1",
                "expires_in": 3599,
                "refresh_token": "refresh-secret-1",
                "scope": "openid https://www.googleapis.com/auth/calendar https://www.googleapis.com/auth/userinfo.email",
                "id_token": id_token(),
            }))
            .into_response()
        }
        Some("refresh_token") => {
            if w.revoked
                || form.get("refresh_token").map(String::as_str) != Some("refresh-secret-1")
            {
                return bad("invalid_grant");
            }
            w.refreshes += 1;
            Json(json!({ "access_token": "access-1", "expires_in": 3599 })).into_response()
        }
        _ => bad("unsupported_grant_type"),
    }
}

async fn revoke(State(fake): State<Fake>) -> StatusCode {
    fake.0.lock().unwrap().revoked = true;
    StatusCode::OK
}

async fn calendar_list(State(fake): State<Fake>, headers: HeaderMap) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let w = fake.0.lock().unwrap();
    let items: Vec<Value> = w
        .calendars
        .iter()
        .map(|(id, summary, owner)| {
            json!({
                "id": id, "summary": summary, "primary": id == PRIMARY,
                "backgroundColor": "#9fe1e7",
                "accessRole": if *owner { "owner" } else { "reader" },
            })
        })
        .collect();
    Json(json!({ "items": items })).into_response()
}

async fn create_calendar(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut w = fake.0.lock().unwrap();
    let summary = body["summary"].as_str().unwrap_or("").to_string();
    w.calendars.push((CREATED.into(), summary.clone(), true));
    Json(json!({ "id": CREATED, "summary": summary })).into_response()
}

async fn list_events(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Path(cal): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let w = fake.0.lock().unwrap();
    if !w.calendars.iter().any(|(id, _, _)| *id == cal) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let events = w.events.get(&cal).cloned().unwrap_or_default();
    let items: Vec<Value> = match q.get("syncToken") {
        Some(t) if t == "expired" => return StatusCode::GONE.into_response(),
        Some(t) => {
            let since: u64 = t.parse().unwrap();
            events
                .iter()
                .filter(|e| e.seq > since)
                .map(FakeEvent::json)
                .collect()
        }
        None => events
            .iter()
            .filter(|e| !e.cancelled)
            .map(FakeEvent::json)
            .collect(),
    };
    let mut page = json!({ "items": items });
    if !q.contains_key("timeMin") {
        page["nextSyncToken"] = json!(w.seq.to_string());
    }
    Json(page).into_response()
}

async fn insert_event(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Path(cal): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut w = fake.0.lock().unwrap();
    let seq = w.bump();
    let event = FakeEvent {
        id: format!("ev{seq}"),
        summary: body["summary"].as_str().unwrap_or("").into(),
        start: body["start"].clone(),
        end: body["end"].clone(),
        cancelled: false,
        etag: format!("\"etag-{seq}\""),
        seq,
        private: body["extendedProperties"]["private"].clone(),
    };
    let out = event.json();
    w.events.entry(cal).or_default().push(event);
    Json(out).into_response()
}

async fn patch_event(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Path((cal, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut w = fake.0.lock().unwrap();
    let seq = w.bump();
    let Some(e) = w
        .events
        .get_mut(&cal)
        .and_then(|es| es.iter_mut().find(|e| e.id == id && !e.cancelled))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    e.summary = body["summary"].as_str().unwrap_or(&e.summary).into();
    e.start = body["start"].clone();
    e.end = body["end"].clone();
    e.seq = seq;
    e.etag = format!("\"etag-{seq}\"");
    Json(e.json()).into_response()
}

async fn delete_event(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Path((cal, id)): Path<(String, String)>,
) -> StatusCode {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let mut w = fake.0.lock().unwrap();
    let seq = w.bump();
    match w
        .events
        .get_mut(&cal)
        .and_then(|es| es.iter_mut().find(|e| e.id == id))
    {
        Some(e) if e.cancelled => StatusCode::GONE,
        Some(e) => {
            e.cancelled = true;
            e.seq = seq;
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

// ---- tests ----------------------------------------------------------------------------------

async fn connected() -> (Client, Fake) {
    let (fake, endpoints) = Fake::start().await;
    let mut c = Client::with_google(Some(Config {
        client_id: "client-1".into(),
        client_secret: "secret".into(),
        key: [42; 32],
        endpoints,
    }))
    .await;

    let start = c
        .ok(Method::POST, "/api/integrations/google/start", None)
        .await;
    let auth = reqwest::Url::parse(start["auth_url"].as_str().unwrap()).unwrap();
    let param = |k: &str| {
        auth.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
            .unwrap()
    };
    assert_eq!(param("code_challenge_method"), "S256");
    assert_eq!(param("access_type"), "offline");
    assert_eq!(param("redirect_uri"), "http://127.0.0.1:8642/");
    assert!(param("scope").contains("auth/calendar"));
    let state = param("state");

    // A paste from an older attempt is refused without spoiling the current one.
    let (status, err) = c
        .call(
            Method::POST,
            "/api/integrations/google/finish",
            Some(json!({ "redirected_url": "http://127.0.0.1:8642/?state=old&code=good-code" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{err}");

    let pasted = format!("http://127.0.0.1:8642/?state={state}&code=good-code&scope=x");
    let s = c
        .ok(
            Method::POST,
            "/api/integrations/google/finish",
            Some(json!({ "redirected_url": pasted })),
        )
        .await;
    assert_eq!(s["connected"], true);
    assert_eq!(s["email"], "adam@example.com");
    (c, fake)
}

async fn task(c: &mut Client, body: Value) -> i64 {
    c.ok(Method::POST, "/api/tasks", Some(body)).await["id"]
        .as_i64()
        .unwrap()
}

async fn pending(c: &mut Client) -> i64 {
    c.ok(Method::GET, "/api/integrations/google/push", None)
        .await["pending"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn google_is_off_without_configuration() {
    let mut c = Client::new().await;
    let s = c.ok(Method::GET, "/api/integrations/google", None).await;
    assert_eq!(s["configured"], false);
    assert_eq!(s["connected"], false);
    let (status, err) = c
        .call(Method::POST, "/api/integrations/google/start", None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(err["error"].as_str().unwrap().contains("google-setup"));
}

#[tokio::test]
async fn connect_stores_the_refresh_token_encrypted() {
    let (mut c, _fake) = connected().await;
    let blob: Vec<u8> = sqlx::query_scalar("SELECT refresh_token_enc FROM google_accounts")
        .fetch_one(&c.db)
        .await
        .unwrap();
    assert!(!blob.windows(14).any(|w| w == b"refresh-secret"));

    let cals = c
        .ok(Method::GET, "/api/integrations/google/calendars", None)
        .await;
    let names: Vec<&str> = cals
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["summary"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Adam", "Holidays"], "primary first");
    assert_eq!(cals[1]["writable"], false);

    // A calendar we can only read can't take our tasks.
    let (status, _) = c
        .call(
            Method::PUT,
            "/api/integrations/google/calendars",
            Some(json!({ "tasks": FAMILY })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn push_on_demand_and_pull_changes_back() {
    let (mut c, fake) = connected().await;
    fake.add(
        PRIMARY,
        "Dentist",
        json!({ "dateTime": "2026-10-21T09:00:00-07:00" }),
        json!({ "dateTime": "2026-10-21T10:00:00-07:00" }),
    );
    fake.add(
        FAMILY,
        "Holiday",
        json!({ "date": "2026-10-12" }),
        json!({ "date": "2026-10-13" }),
    );
    let s = c
        .ok(
            Method::PUT,
            "/api/integrations/google/calendars",
            Some(json!({ "tasks": "new", "overlays": [PRIMARY, FAMILY] })),
        )
        .await;
    let roles: Vec<(&str, &str)> = s["calendars"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["summary"].as_str().unwrap(), c["role"].as_str().unwrap()))
        .collect();
    assert_eq!(
        roles,
        [
            ("Kanban", "tasks"),
            ("Adam", "overlay"),
            ("Holidays", "overlay")
        ]
    );
    c.ok(Method::POST, "/api/integrations/google/sync", None)
        .await;

    let p = c
        .ok(
            Method::POST,
            "/api/projects",
            Some(json!({ "name": "Home" })),
        )
        .await;
    let pid = p["id"].as_i64().unwrap();
    let mine = task(&mut c, json!({ "project_id": pid, "title": "Pay rent", "assignee_id": 1, "due_date": "2026-10-20" })).await;
    // Not pushed: unassigned, someone else's, or without a date.
    task(
        &mut c,
        json!({ "project_id": pid, "title": "Unassigned", "due_date": "2026-10-20" }),
    )
    .await;
    task(
        &mut c,
        json!({ "project_id": pid, "title": "Kat's", "assignee_id": 2, "due_date": "2026-10-20" }),
    )
    .await;
    task(
        &mut c,
        json!({ "project_id": pid, "title": "Someday", "assignee_id": 1 }),
    )
    .await;

    // Nothing reaches Google until the button.
    assert_eq!(pending(&mut c).await, 1);
    assert!(fake.events(CREATED).is_empty());
    let r = c
        .ok(Method::POST, "/api/integrations/google/push", None)
        .await;
    assert_eq!(r["report"]["created"], 1);
    assert_eq!(r["status"]["pending"], 0);
    let events = fake.events(CREATED);
    assert_eq!(events.len(), 1);
    let ev = events[0].clone();
    assert_eq!(ev.summary, "Pay rent");
    assert_eq!(ev.start["date"], "2026-10-20");
    assert_eq!(ev.end["date"], "2026-10-21", "all-day end is exclusive");
    assert_eq!(ev.private["kanbanTaskId"], mine.to_string());

    // Our own write coming back on the next pull changes nothing.
    let r = c
        .ok(Method::POST, "/api/integrations/google/sync", None)
        .await;
    assert_eq!(r["report"]["tasks_changed"], 0);

    // Moved in Google Calendar: the due date follows, and the history says where it came from.
    fake.edit(CREATED, &ev.id, |e| {
        e.start = json!({ "date": "2026-10-23" });
        e.end = json!({ "date": "2026-10-24" });
    });
    let r = c
        .ok(Method::POST, "/api/integrations/google/sync", None)
        .await;
    assert_eq!(r["report"]["tasks_changed"], 1);
    let t = c.ok(Method::GET, &format!("/api/tasks/{mine}"), None).await;
    assert_eq!(t["due_date"], "2026-10-23");
    assert_eq!(pending(&mut c).await, 0);
    let history = c
        .ok(Method::GET, &format!("/api/tasks/{mine}/activity"), None)
        .await;
    let last = history.as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["kind"].as_str(), last["to_value"].as_str()),
        (Some("due_google"), Some("2026-10-23"))
    );

    // Renamed and completed in the app: waits for the button, then updates the same event.
    c.ok(
        Method::PATCH,
        &format!("/api/tasks/{mine}"),
        Some(json!({ "title": "Pay October rent", "completed": true })),
    )
    .await;
    assert_eq!(pending(&mut c).await, 1);
    let r = c
        .ok(Method::POST, "/api/integrations/google/push", None)
        .await;
    assert_eq!(r["report"]["updated"], 1);
    let events = fake.events(CREATED);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].summary, "✓ Pay October rent");
    assert_eq!(events[0].start["date"], "2026-10-23");

    // A title edited in Google counts as a change, and the push puts the app's title back.
    fake.edit(CREATED, &ev.id, |e| e.summary = "renamed in google".into());
    c.ok(Method::POST, "/api/integrations/google/sync", None)
        .await;
    assert_eq!(pending(&mut c).await, 1);
    c.ok(Method::POST, "/api/integrations/google/push", None)
        .await;
    assert_eq!(fake.events(CREATED)[0].summary, "✓ Pay October rent");

    // Deleted in Google: the task stays, without a due date, and isn't pushed again.
    fake.edit(CREATED, &ev.id, |e| e.cancelled = true);
    c.ok(Method::POST, "/api/integrations/google/sync", None)
        .await;
    let t = c.ok(Method::GET, &format!("/api/tasks/{mine}"), None).await;
    assert_eq!(t["due_date"], Value::Null);
    assert_eq!(pending(&mut c).await, 0);

    // Deleting a pushed task in the app deletes its event on the next push.
    let gone = task(&mut c, json!({ "project_id": pid, "title": "Book flights", "assignee_id": 1, "due_date": "2026-11-02" })).await;
    c.ok(Method::POST, "/api/integrations/google/push", None)
        .await;
    assert_eq!(
        fake.events(CREATED).iter().filter(|e| !e.cancelled).count(),
        1
    );
    c.ok(Method::DELETE, &format!("/api/tasks/{gone}"), None)
        .await;
    assert_eq!(pending(&mut c).await, 1);
    let r = c
        .ok(Method::POST, "/api/integrations/google/push", None)
        .await;
    assert_eq!(r["report"]["deleted"], 1);
    assert_eq!(
        fake.events(CREATED).iter().filter(|e| !e.cancelled).count(),
        0
    );

    // The Calendar view: tasks by due date, and the overlay events beside them.
    let cal = c
        .ok(
            Method::GET,
            "/api/calendar?from=2026-10-01&to=2026-10-31",
            None,
        )
        .await;
    let titles: Vec<&str> = cal["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Unassigned", "Kat's"]);
    let events: Vec<(&str, &str, bool)> = cal["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["summary"].as_str().unwrap(),
                e["calendar"].as_str().unwrap(),
                e["all_day"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        events,
        [("Holiday", "Holidays", true), ("Dentist", "Adam", false)]
    );
    let none = c
        .ok(
            Method::GET,
            "/api/calendar?from=2027-01-01&to=2027-01-31",
            None,
        )
        .await;
    assert_eq!(none["events"].as_array().unwrap().len(), 0);

    // Each person sees only their own Google calendars.
    c.login_as("kat").await;
    let kat = c
        .ok(
            Method::GET,
            "/api/calendar?from=2026-10-01&to=2026-10-31",
            None,
        )
        .await;
    assert_eq!(kat["events"].as_array().unwrap().len(), 0);
    let s = c.ok(Method::GET, "/api/integrations/google", None).await;
    assert_eq!(s["connected"], false);
}

#[tokio::test]
async fn revoked_access_is_reported_and_disconnect_cleans_up() {
    let (mut c, fake) = connected().await;
    c.ok(
        Method::PUT,
        "/api/integrations/google/calendars",
        Some(json!({ "tasks": "new", "overlays": [PRIMARY] })),
    )
    .await;
    let p = c
        .ok(
            Method::POST,
            "/api/projects",
            Some(json!({ "name": "Home" })),
        )
        .await;
    task(&mut c, json!({ "project_id": p["id"], "title": "Taxes", "assignee_id": 1, "due_date": "2026-10-20" })).await;
    c.ok(Method::POST, "/api/integrations/google/push", None)
        .await;

    // Disconnect: revoked at Google, and nothing of the connection is left here.
    let (status, _) = c
        .call(Method::DELETE, "/api/integrations/google", None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(fake.0.lock().unwrap().revoked);
    for table in [
        "google_accounts",
        "google_calendars",
        "task_events",
        "calendar_events",
    ] {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&c.db)
                .await
                .unwrap();
        assert_eq!(n, 0, "{table}");
    }

    // Reconnect, then lose access in Google: the sync says so in plain words.
    let (mut c2, fake2) = connected().await;
    c2.ok(
        Method::PUT,
        "/api/integrations/google/calendars",
        Some(json!({ "tasks": "new" })),
    )
    .await;
    fake2.0.lock().unwrap().revoked = true;
    // Force a token refresh, as happens after an hour.
    let user: i64 = 1;
    c2_forget_token(&c2, user);
    let (status, err) = c2
        .call(Method::POST, "/api/integrations/google/sync", None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"], "Google access was revoked. Reconnect.");
    let s = c2.ok(Method::GET, "/api/integrations/google", None).await;
    assert_eq!(s["last_error"], "Google access was revoked. Reconnect.");
}

fn c2_forget_token(c: &Client, user: i64) {
    c.google.forget_access_token(user);
}
