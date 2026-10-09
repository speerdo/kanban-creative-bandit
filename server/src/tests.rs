//! End-to-end API tests: the real router against a fresh SQLite file per test.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{AppState, app, auth, db};

mod google_sync;

struct Client {
    app: Router,
    db: sqlx::SqlitePool,
    google: std::sync::Arc<crate::google::Google>,
    cookie: Option<String>,
    events: crate::events::Hub,
    /// Dropping this would end every event stream.
    _stop: tokio::sync::watch::Sender<bool>,
    _dir: TempDir,
}

/// A throwaway directory removed on drop (avoids a dev-dependency for one helper).
struct TempDir(std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Client {
    /// A new database with users `adam` and `kat` (password `password1`), signed in as adam.
    async fn new() -> Self {
        Self::with_google(None).await
    }

    /// The same, with the Google integration configured (pointing at a stand-in server).
    async fn with_google(google: Option<crate::google::Config>) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "kanban-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pool = db::connect(&dir.join("test.db")).await.unwrap();
        let hash = auth::hash_password("password1").unwrap();
        for (u, d) in [("adam", "Adam"), ("kat", "Kat")] {
            sqlx::query(
                "INSERT INTO users (username, display_name, password_hash) VALUES (?, ?, ?)",
            )
            .bind(u)
            .bind(d)
            .bind(&hash)
            .execute(&pool)
            .await
            .unwrap();
        }
        let (events, stop) = crate::events::Hub::new();
        let google = std::sync::Arc::new(crate::google::Google::new(google).unwrap());
        let mut c = Self {
            app: app(AppState {
                db: pool.clone(),
                events: events.clone(),
                google: google.clone(),
            }),
            db: pool,
            google,
            cookie: None,
            events,
            _stop: stop,
            _dir: TempDir(dir),
        };
        let (status, _) = c
            .call(
                Method::POST,
                "/api/auth/login",
                Some(json!({"username": "Adam", "password": "password1"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(c.cookie.is_some());
        c
    }

    async fn call(
        &mut self,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(cookie) = &self.cookie {
            req = req.header(header::COOKIE, cookie);
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();

        let res = self.app.clone().oneshot(req).await.unwrap();
        if let Some(set) = res.headers().get(header::SET_COOKIE) {
            let pair = set.to_str().unwrap().split(';').next().unwrap().to_string();
            self.cookie = Some(pair);
        }
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    async fn login_as(&mut self, username: &str) {
        let body = json!({"username": username, "password": "password1"});
        let (status, _) = self.call(Method::POST, "/api/auth/login", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
    }

    async fn ok(&mut self, method: Method, uri: &str, body: Option<Value>) -> Value {
        let (status, v) = self.call(method, uri, body).await;
        assert!(status.is_success(), "{uri}: {status} {v}");
        v
    }
}

fn titles(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn auth_required_and_wrong_password_rejected() {
    let mut c = Client::new().await;
    let (status, me) = c.call(Method::GET, "/api/me", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["username"], "adam");

    c.call(Method::POST, "/api/auth/logout", None).await;
    let (status, _) = c.call(Method::GET, "/api/projects", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = c
        .call(
            Method::POST,
            "/api/auth/login",
            Some(json!({"username": "adam", "password": "nope"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = c
        .call(
            Method::POST,
            "/api/auth/login",
            Some(json!({"username": "ghost", "password": "nope"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn project_gets_default_statuses() {
    let mut c = Client::new().await;
    let p = c
        .ok(
            Method::POST,
            "/api/projects",
            Some(json!({"name": " Website ", "color": "teal"})),
        )
        .await;
    assert_eq!(p["name"], "Website");
    let s = c
        .ok(
            Method::GET,
            &format!("/api/projects/{}/statuses", p["id"]),
            None,
        )
        .await;
    let names: Vec<_> = s
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Backlog", "To Do", "In Progress", "Review", "Done"]);

    let (status, _) = c
        .call(
            Method::POST,
            "/api/projects",
            Some(json!({"name": "x", "color": "chartreuse"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn tasks_order_move_and_complete() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "Ops"})))
        .await;
    let pid = p["id"].as_i64().unwrap();
    let statuses = c
        .ok(Method::GET, &format!("/api/projects/{pid}/statuses"), None)
        .await;
    let backlog = statuses[0]["id"].as_i64().unwrap();
    let doing = statuses[2]["id"].as_i64().unwrap();
    let done = statuses[4]["id"].as_i64().unwrap();

    let mut ids = vec![];
    for title in ["a", "b", "c"] {
        let t = c
            .ok(
                Method::POST,
                "/api/tasks",
                Some(json!({"project_id": pid, "title": title})),
            )
            .await;
        assert_eq!(t["status_id"], backlog);
        ids.push(t["id"].as_i64().unwrap());
    }
    let list = c
        .ok(Method::GET, &format!("/api/tasks?project={pid}"), None)
        .await;
    assert_eq!(titles(&list), ["a", "b", "c"]);

    // Reorder: c before a.
    c.ok(
        Method::POST,
        &format!("/api/tasks/{}/move", ids[2]),
        Some(json!({"before_id": ids[0]})),
    )
    .await;
    // Insert between c and a.
    c.ok(
        Method::POST,
        "/api/tasks",
        Some(json!({"project_id": pid, "title": "ca", "after_id": ids[2]})),
    )
    .await;
    let list = c
        .ok(Method::GET, &format!("/api/tasks?project={pid}"), None)
        .await;
    assert_eq!(titles(&list), ["c", "ca", "a", "b"]);

    // Move across columns, then complete via the checkbox shortcut.
    let t = c
        .ok(
            Method::POST,
            &format!("/api/tasks/{}/move", ids[1]),
            Some(json!({"status_id": doing})),
        )
        .await;
    assert_eq!(t["status_id"], doing);
    assert!(t["completed_at"].is_null());
    let t = c
        .ok(
            Method::PATCH,
            &format!("/api/tasks/{}", ids[1]),
            Some(json!({"completed": true})),
        )
        .await;
    assert_eq!(t["status_id"], done);
    assert!(t["completed_at"].is_string());
    let t = c
        .ok(
            Method::PATCH,
            &format!("/api/tasks/{}", ids[1]),
            Some(json!({"completed": false})),
        )
        .await;
    assert_eq!(t["status_id"], statuses[1]["id"], "reopens into 'To Do'");
    assert!(t["completed_at"].is_null());

    // Other projects' statuses are refused.
    let other = c
        .ok(
            Method::POST,
            "/api/projects",
            Some(json!({"name": "Other"})),
        )
        .await;
    let other_s = c
        .ok(
            Method::GET,
            &format!("/api/projects/{}/statuses", other["id"]),
            None,
        )
        .await;
    let (status, _) = c
        .call(
            Method::POST,
            &format!("/api/tasks/{}/move", ids[0]),
            Some(json!({"status_id": other_s[0]["id"]})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn patch_distinguishes_missing_from_null() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let me = c.ok(Method::GET, "/api/me", None).await;
    let t = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(
                json!({"project_id": p["id"], "title": "t", "due_date": "2026-10-31",
                        "assignee_id": me["id"], "priority": "high"}),
            ),
        )
        .await;
    let url = format!("/api/tasks/{}", t["id"]);

    let t = c
        .ok(Method::PATCH, &url, Some(json!({"title": "renamed"})))
        .await;
    assert_eq!(t["due_date"], "2026-10-31");
    assert_eq!(t["assignee_id"], me["id"]);

    let t = c
        .ok(
            Method::PATCH,
            &url,
            Some(json!({"due_date": null, "assignee_id": null})),
        )
        .await;
    assert!(t["due_date"].is_null());
    assert!(t["assignee_id"].is_null());

    let (status, _) = c
        .call(Method::PATCH, &url, Some(json!({"due_date": "31/10/2026"})))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let mine = c.ok(Method::GET, "/api/tasks?assignee=me", None).await;
    assert_eq!(mine.as_array().unwrap().len(), 0);
    let search = c.ok(Method::GET, "/api/tasks?q=NAME", None).await;
    assert_eq!(titles(&search), ["renamed"]);
}

#[tokio::test]
async fn deleting_a_status_moves_its_tasks() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let pid = p["id"].as_i64().unwrap();
    let s = c
        .ok(Method::GET, &format!("/api/projects/{pid}/statuses"), None)
        .await;
    let (backlog, done) = (s[0]["id"].as_i64().unwrap(), s[4]["id"].as_i64().unwrap());
    c.ok(
        Method::POST,
        "/api/tasks",
        Some(json!({"project_id": pid, "title": "x"})),
    )
    .await;

    let (status, _) = c
        .call(Method::DELETE, &format!("/api/statuses/{backlog}"), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    c.ok(
        Method::DELETE,
        &format!("/api/statuses/{backlog}?move_to={done}"),
        None,
    )
    .await;

    let list = c
        .ok(Method::GET, &format!("/api/tasks?project={pid}"), None)
        .await;
    assert_eq!(list[0]["status_id"], done);
    assert!(list[0]["completed_at"].is_string());

    // Deleting the whole project takes its tasks with it.
    c.ok(Method::DELETE, &format!("/api/projects/{pid}"), None)
        .await;
    let list = c.ok(Method::GET, "/api/tasks", None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn subtasks_are_listed_under_their_parent() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let parent = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(json!({"project_id": p["id"], "title": "parent"})),
        )
        .await;
    c.ok(
        Method::POST,
        "/api/tasks",
        Some(json!({"parent_task_id": parent["id"], "title": "child"})),
    )
    .await;

    let top = c.ok(Method::GET, "/api/tasks", None).await;
    assert_eq!(titles(&top), ["parent"]);
    assert_eq!(top[0]["subtask_count"], 1);
    let subs = c
        .ok(
            Method::GET,
            &format!("/api/tasks?parent={}", parent["id"]),
            None,
        )
        .await;
    assert_eq!(titles(&subs), ["child"]);
}

#[tokio::test]
async fn changes_are_broadcast() {
    let mut c = Client::new().await;
    let mut rx = c.events.subscribe();
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let parent = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(json!({"project_id": p["id"], "title": "parent"})),
        )
        .await;
    let child = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(json!({"parent_task_id": parent["id"], "title": "child"})),
        )
        .await;
    c.ok(Method::DELETE, &format!("/api/tasks/{}", child["id"]), None)
        .await;

    let mut seen = vec![];
    while let Ok(e) = rx.try_recv() {
        assert_eq!(e.by, 1, "adam made every change");
        seen.push((e.kind, e.data["id"].as_i64()));
    }
    let (pid, parent_id, child_id) = (
        p["id"].as_i64(),
        parent["id"].as_i64(),
        child["id"].as_i64(),
    );
    assert_eq!(
        seen,
        [
            ("project.created", pid),
            ("task.created", parent_id),
            ("task.created", child_id),
            ("task.updated", parent_id), // its subtask count changed
            ("task.deleted", child_id),
            ("task.updated", parent_id),
        ]
    );
}

#[tokio::test]
async fn event_stream_requires_sign_in_and_says_hello() {
    let mut c = Client::new().await;
    let req = Request::get("/api/events")
        .header(header::COOKIE, c.cookie.clone().unwrap())
        .body(Body::empty())
        .unwrap();
    let res = c.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "text/event-stream");
    let mut body = res.into_body();
    let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert!(String::from_utf8_lossy(&frame).starts_with("event: hello"));

    c.call(Method::POST, "/api/auth/logout", None).await;
    let (status, _) = c.call(Method::GET, "/api/events", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn prefs_default_merge_and_validate() {
    let mut c = Client::new().await;
    let me = c.ok(Method::GET, "/api/me", None).await;
    assert_eq!(me["username"], "adam");
    assert_eq!(me["prefs"]["theme"], "system");
    assert!(me["prefs"]["background_color"].is_null());

    let mut rx = c.events.subscribe();
    let p = c
        .ok(
            Method::PUT,
            "/api/me/prefs",
            Some(json!({"theme": "dark", "accent_color": "#E8451F", "background_color": "teal"})),
        )
        .await;
    assert_eq!(p["theme"], "dark");
    assert_eq!(p["accent_color"], "#e8451f", "hex is normalised");
    assert_eq!(
        p["density"], "comfortable",
        "untouched fields keep their value"
    );
    assert_eq!(rx.try_recv().unwrap().kind, "prefs.updated");

    // Only the fields sent change; null clears the background.
    let p = c
        .ok(
            Method::PUT,
            "/api/me/prefs",
            Some(json!({"background_color": null, "density": "compact"})),
        )
        .await;
    assert!(p["background_color"].is_null());
    assert_eq!(p["theme"], "dark");
    assert_eq!(p["density"], "compact");

    for bad in [
        json!({"theme": "neon"}),
        json!({"accent_color": "#12345"}),
        json!({"accent_color": "chartreuse"}),
    ] {
        let (status, _) = c.call(Method::PUT, "/api/me/prefs", Some(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // Login hands back the prefs too, so the UI can theme itself straight away.
    let (_, login) = c
        .call(
            Method::POST,
            "/api/auth/login",
            Some(json!({"username": "adam", "password": "password1"})),
        )
        .await;
    assert_eq!(login["prefs"]["theme"], "dark");
}

#[tokio::test]
async fn profile_update_is_broadcast() {
    let mut c = Client::new().await;
    let mut rx = c.events.subscribe();
    let me = c
        .ok(
            Method::PATCH,
            "/api/me",
            Some(json!({"display_name": " Adam S ", "avatar_color": "#1b27e8"})),
        )
        .await;
    assert_eq!(me["display_name"], "Adam S");
    assert_eq!(me["avatar_color"], "#1b27e8");
    let e = rx.try_recv().unwrap();
    assert_eq!(
        (e.kind, e.data["display_name"].as_str()),
        ("user.updated", Some("Adam S"))
    );

    // Custom hex colors work for projects too.
    let p = c
        .ok(
            Method::POST,
            "/api/projects",
            Some(json!({"name": "Custom", "color": "#FFE800"})),
        )
        .await;
    assert_eq!(p["color"], "#ffe800");
}

#[tokio::test]
async fn labels_on_tasks_filter_and_history() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let bug = c
        .ok(
            Method::POST,
            "/api/labels",
            Some(json!({"name": "bug", "color": "red"})),
        )
        .await;
    let garden = c
        .ok(
            Method::POST,
            "/api/labels",
            Some(json!({"name": "Garden", "color": "green"})),
        )
        .await;
    let (status, _) = c
        .call(Method::POST, "/api/labels", Some(json!({"name": "BUG"})))
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "names are unique, ignoring case"
    );

    let t = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(
                json!({"project_id": p["id"], "title": "weed beds", "label_ids": [garden["id"]],
                        "description": "the raised ones by the shed"}),
            ),
        )
        .await;
    assert_eq!(t["label_ids"], json!([garden["id"]]));
    let url = format!("/api/tasks/{}", t["id"]);

    let me = c.ok(Method::GET, "/api/me", None).await;
    let statuses = c.ok(Method::GET, "/api/statuses", None).await;
    c.ok(
        Method::PATCH,
        &url,
        Some(
            json!({"label_ids": [bug["id"], garden["id"]], "priority": "high",
                    "assignee_id": me["id"], "status_id": statuses[2]["id"]}),
        ),
    )
    .await;
    c.ok(Method::PATCH, &url, Some(json!({"completed": true})))
        .await;

    let by_label = c
        .ok(
            Method::GET,
            &format!("/api/tasks?label={}", bug["id"]),
            None,
        )
        .await;
    assert_eq!(titles(&by_label), ["weed beds"]);
    let found = c.ok(Method::GET, "/api/tasks?q=SHED", None).await;
    assert_eq!(titles(&found), ["weed beds"], "search covers descriptions");

    let log = c.ok(Method::GET, &format!("{url}/activity"), None).await;
    let entries: Vec<(&str, Option<&str>, Option<&str>)> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["kind"].as_str().unwrap(),
                a["from_value"].as_str(),
                a["to_value"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            ("created", None, None),
            ("assignee", None, Some("Adam")),
            ("priority", Some("none"), Some("high")),
            ("labels", Some("Garden"), Some("bug, Garden")),
            ("status", Some("Backlog"), Some("In Progress")),
            ("status", Some("In Progress"), Some("Done")),
            ("completed", None, None),
        ]
    );

    // Deleting a label takes it off its tasks.
    c.ok(Method::DELETE, &format!("/api/labels/{}", bug["id"]), None)
        .await;
    let t = c.ok(Method::GET, &url, None).await;
    assert_eq!(t["label_ids"], json!([garden["id"]]));
}

#[tokio::test]
async fn comments_belong_to_their_author() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let t = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(json!({"project_id": p["id"], "title": "t"})),
        )
        .await;
    let url = format!("/api/tasks/{}/comments", t["id"]);
    let comment = c
        .ok(
            Method::POST,
            &url,
            Some(json!({"body": "  **looks good**  "})),
        )
        .await;
    assert_eq!(comment["body"], "**looks good**");
    assert_eq!(comment["project_id"], p["id"]);
    let (status, _) = c
        .call(Method::POST, &url, Some(json!({"body": "   "})))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    c.login_as("kat").await;
    let curl = format!("/api/comments/{}", comment["id"]);
    let (status, _) = c
        .call(Method::PATCH, &curl, Some(json!({"body": "hijack"})))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = c.call(Method::DELETE, &curl, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    c.ok(Method::POST, &url, Some(json!({"body": "agreed"})))
        .await;

    c.login_as("adam").await;
    let edited = c
        .ok(Method::PATCH, &curl, Some(json!({"body": "looks great"})))
        .await;
    assert!(edited["edited_at"].is_string());
    let all = c.ok(Method::GET, &url, None).await;
    let bodies: Vec<_> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, ["looks great", "agreed"]);
    c.ok(Method::DELETE, &curl, None).await;

    let (status, _) = c
        .call(
            Method::POST,
            "/api/tasks/9999/comments",
            Some(json!({"body": "x"})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn my_tasks_include_subtasks_when_asked() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let me = c.ok(Method::GET, "/api/me", None).await;
    let parent = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(json!({"project_id": p["id"], "title": "parent"})),
        )
        .await;
    c.ok(
        Method::POST,
        "/api/tasks",
        Some(json!({"parent_task_id": parent["id"], "title": "mine", "assignee_id": me["id"]})),
    )
    .await;
    let top = c.ok(Method::GET, "/api/tasks?assignee=me", None).await;
    assert_eq!(titles(&top).len(), 0);
    let all = c
        .ok(
            Method::GET,
            "/api/tasks?assignee=me&any_level=true&completed=false",
            None,
        )
        .await;
    assert_eq!(titles(&all), ["mine"]);
}

#[tokio::test]
async fn shared_lists_reuse_items_and_clear_checked() {
    let mut c = Client::new().await;
    let mut events = c.events.subscribe();
    let list = c
        .ok(
            Method::POST,
            "/api/lists",
            Some(json!({"name": "Groceries"})),
        )
        .await;
    let id = list["id"].as_i64().unwrap();
    assert_eq!(list["open_count"], 0);

    // A pasted list becomes one item per line.
    let added = c
        .ok(
            Method::POST,
            &format!("/api/lists/{id}/items"),
            Some(json!({"text": "Milk\n- Eggs\n\nBread"})),
        )
        .await;
    assert_eq!(added.as_array().unwrap().len(), 3);
    let items = c
        .ok(Method::GET, &format!("/api/lists/{id}/items"), None)
        .await;
    let texts: Vec<&str> = items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["Milk", "Eggs", "Bread"]);

    // Kat checks off milk; it's live for Adam's phone.
    c.login_as("kat").await;
    let milk = items[0]["id"].as_i64().unwrap();
    let checked = c
        .ok(
            Method::PATCH,
            &format!("/api/list-items/{milk}"),
            Some(json!({"checked": true})),
        )
        .await;
    assert!(checked["checked_at"].is_string());
    assert_eq!(checked["checked_by"], 2);
    let mut kinds = vec![];
    while let Ok(e) = events.try_recv() {
        kinds.push(e.kind);
    }
    assert!(kinds.contains(&"list_item.updated") && kinds.contains(&"list.updated"));

    // Adding "milk" next week brings the same item back instead of a duplicate.
    let again = c
        .ok(
            Method::POST,
            &format!("/api/lists/{id}/items"),
            Some(json!({"text": "milk"})),
        )
        .await;
    assert_eq!(again[0]["id"], milk);
    assert_eq!(again[0]["checked_at"], Value::Null);
    let lists = c.ok(Method::GET, "/api/lists", None).await;
    assert_eq!(lists[0]["open_count"], 3);

    // Check two, clear checked: only the open one is left.
    for item in &items.as_array().unwrap()[..2] {
        c.ok(
            Method::PATCH,
            &format!("/api/list-items/{}", item["id"]),
            Some(json!({"checked": true})),
        )
        .await;
    }
    let r = c
        .ok(
            Method::POST,
            &format!("/api/lists/{id}/clear-checked"),
            None,
        )
        .await;
    assert_eq!(r["removed"], 2);
    let items = c
        .ok(Method::GET, &format!("/api/lists/{id}/items"), None)
        .await;
    assert_eq!(items[0]["text"], "Bread");
    assert_eq!(items.as_array().unwrap().len(), 1);

    // Reorder and rename.
    let bread = items[0]["id"].as_i64().unwrap();
    let added = c
        .ok(
            Method::POST,
            &format!("/api/lists/{id}/items"),
            Some(json!({"text": "Apples"})),
        )
        .await;
    c.ok(
        Method::PATCH,
        &format!("/api/list-items/{}", added[0]["id"]),
        Some(json!({"before_id": bread, "text": "Green apples"})),
    )
    .await;
    let items = c
        .ok(Method::GET, &format!("/api/lists/{id}/items"), None)
        .await;
    assert_eq!(items[0]["text"], "Green apples");

    let (status, _) = c
        .call(
            Method::POST,
            &format!("/api/lists/{id}/items"),
            Some(json!({"text": "  "})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = c
        .call(Method::DELETE, &format!("/api/lists/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = c
        .call(Method::GET, &format!("/api/lists/{id}/items"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn task_links_by_kind() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "P"})))
        .await;
    let t = c
        .ok(
            Method::POST,
            "/api/tasks",
            Some(json!({"project_id": p["id"], "title": "T"})),
        )
        .await;
    let path = format!("/api/tasks/{}/links", t["id"]);
    let doc = c
        .ok(
            Method::POST,
            &path,
            Some(json!({"url": "https://docs.google.com/document/d/abc/edit", "title": "Brief"})),
        )
        .await;
    assert_eq!(
        (
            doc["kind"].as_str(),
            doc["external_id"].as_str(),
            doc["title"].as_str()
        ),
        (Some("doc"), Some("abc"), Some("Brief"))
    );
    c.ok(
        Method::POST,
        &path,
        Some(json!({"url": "https://example.com/x"})),
    )
    .await;
    let (status, _) = c
        .call(
            Method::POST,
            &path,
            Some(json!({"url": "javascript:alert(1)"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let links = c.ok(Method::GET, &path, None).await;
    assert_eq!(links.as_array().unwrap().len(), 2);
    let (status, _) = c
        .call(Method::DELETE, &format!("{path}/{}", doc["id"]), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let links = c.ok(Method::GET, &path, None).await;
    assert_eq!(links[0]["kind"], "url");
}

#[tokio::test]
async fn keep_import_dry_run_then_real() {
    let mut c = Client::new().await;
    let p = c
        .ok(Method::POST, "/api/projects", Some(json!({"name": "Home"})))
        .await;
    c.ok(
        Method::POST,
        "/api/lists",
        Some(json!({"name": "groceries"})),
    )
    .await;
    let notes = json!([
        {"title": "Groceries", "items": [{"text": "Milk"}, {"text": "Rice", "checked": true}]},
        {"title": "Hardware store", "items": [{"text": "Screws"}]},
        {"title": "", "text": "Call the vet\nAsk about the shots"},
        {"title": "Old idea", "text": "x", "archived": true},
        {"title": "Gone", "text": "x", "trashed": true},
    ]);
    let body = |dry: bool| json!({"notes": notes, "project_id": p["id"], "dry_run": dry});

    let r = c
        .ok(Method::POST, "/api/import/keep", Some(body(true)))
        .await;
    assert_eq!(r["dry_run"], true);
    assert_eq!(r["lists"][0]["merged"], true, "same name, any case");
    assert_eq!(r["lists"][1]["merged"], false);
    assert_eq!(r["tasks"], json!(["Call the vet"]));
    assert_eq!(r["skipped"], 2);
    // Nothing changed.
    let lists = c.ok(Method::GET, "/api/lists", None).await;
    assert_eq!(lists.as_array().unwrap().len(), 1);

    c.ok(Method::POST, "/api/import/keep", Some(body(false)))
        .await;
    let lists = c.ok(Method::GET, "/api/lists", None).await;
    let names: Vec<(&str, i64)> = lists
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["name"].as_str().unwrap(),
                l["open_count"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        names,
        [("groceries", 1), ("Hardware store", 1)],
        "checked items stay checked"
    );
    let tasks = c
        .ok(
            Method::GET,
            &format!("/api/tasks?project={}", p["id"]),
            None,
        )
        .await;
    assert_eq!(tasks[0]["title"], "Call the vet");
    assert_eq!(tasks[0]["description"], "Ask about the shots");
}
