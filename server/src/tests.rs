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
        let mut c = Self {
            app: app(AppState { db: pool }),
            cookie: None,
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
