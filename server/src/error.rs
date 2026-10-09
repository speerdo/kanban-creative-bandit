use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    NotFound,
    Conflict(String),
    /// Another service (Google) failed; the message is safe to show.
    Upstream(String),
    Internal(anyhow::Error),
}

impl AppError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m) | Self::Conflict(m) | Self::Upstream(m) => f.write_str(m),
            Self::Unauthorized => f.write_str("not signed in"),
            Self::Forbidden(m) => f.write_str(m),
            Self::NotFound => f.write_str("not found"),
            Self::Internal(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "not signed in".into()),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
            Self::Conflict(m) => (StatusCode::CONFLICT, m),
            Self::Upstream(m) => (StatusCode::BAD_GATEWAY, m),
            Self::Internal(e) => {
                tracing::error!("{e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                Self::bad("refers to something that doesn't exist")
            }
            sqlx::Error::Database(db) if db.is_check_violation() => Self::bad("invalid value"),
            _ => Self::Internal(e.into()),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e)
    }
}
