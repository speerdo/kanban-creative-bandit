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

struct Client {
    app: Router,
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
        let mut c = Self {
            app: app(AppState {
                db: pool,
                events: events.clone(),
            }),
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
