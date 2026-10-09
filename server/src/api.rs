use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;

use crate::{AppState, auth, events, prefs, routes};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/events", get(events::stream))
        .merge(auth::router())
        .merge(prefs::router())
        .merge(routes::router())
        .fallback(not_found)
}

async fn health(State(state): State<AppState>) -> Response {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "db": if db_ok { "ok" } else { "error" },
    });
    (status, Json(body)).into_response()
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
}
