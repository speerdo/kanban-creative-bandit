//! The signed-in user's profile and appearance settings.
//!
//! `GET /api/me` returns the user with their prefs; `PUT /api/me/prefs` merges whichever
//! fields are sent; `PATCH /api/me` changes the profile (display name, avatar color). Changes are
//! broadcast so the same person's other devices restyle live, and so the other person sees a
//! new name or avatar color.

use axum::{
    Json, Router,
    extract::State,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    AppState,
    auth::{CurrentUser, User, get_user},
    error::AppResult,
    validate,
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Prefs {
    pub theme: String,
    pub accent_color: String,
    pub background_color: Option<String>,
    pub background_style: String,
    pub density: String,
    pub default_view: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            accent_color: "blue".into(),
            background_color: None,
            background_style: "solid".into(),
            density: "comfortable".into(),
            default_view: "list".into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Me {
    #[serde(flatten)]
    pub user: User,
    pub prefs: Prefs,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me).patch(update_profile))
        .route("/me/prefs", put(update_prefs))
}

pub async fn load(db: &SqlitePool, user_id: i64) -> AppResult<Prefs> {
    let row = sqlx::query_as(
        "SELECT theme, accent_color, background_color, background_style, density, default_view
         FROM user_prefs WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(row.unwrap_or_default())
}

pub async fn me_for(db: &SqlitePool, user: User) -> AppResult<Me> {
    let prefs = load(db, user.id).await?;
    Ok(Me { user, prefs })
}

async fn me(State(state): State<AppState>, CurrentUser(user): CurrentUser) -> AppResult<Json<Me>> {
    Ok(Json(me_for(&state.db, user).await?))
}

#[derive(Deserialize)]
struct PrefsBody {
    theme: Option<String>,
    accent_color: Option<String>,
    #[serde(default, deserialize_with = "validate::nullable")]
    background_color: Option<Option<String>>,
    background_style: Option<String>,
    density: Option<String>,
    default_view: Option<String>,
}

async fn update_prefs(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<PrefsBody>,
) -> AppResult<Json<Prefs>> {
    let mut p = load(&state.db, me.id).await?;
    if let Some(v) = body.theme {
        p.theme = validate::one_of("theme", &v, &["system", "light", "dark"])?;
    }
    if let Some(v) = body.accent_color {
        p.accent_color = validate::color(&v)?;
    }
    if let Some(v) = body.background_color {
        p.background_color = v.as_deref().map(validate::color).transpose()?;
    }
    if let Some(v) = body.background_style {
        p.background_style = validate::one_of(
            "background_style",
            &v,
            &["solid", "gradient", "subtle-pattern"],
        )?;
    }
    if let Some(v) = body.density {
        p.density = validate::one_of("density", &v, &["comfortable", "compact"])?;
    }
    if let Some(v) = body.default_view {
        p.default_view = validate::one_of("default_view", &v, &["list", "board"])?;
    }

    sqlx::query(
        "INSERT INTO user_prefs (user_id, theme, accent_color, background_color, background_style,
             density, default_view)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (user_id) DO UPDATE SET
             theme = excluded.theme, accent_color = excluded.accent_color,
             background_color = excluded.background_color,
             background_style = excluded.background_style, density = excluded.density,
             default_view = excluded.default_view,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )
    .bind(me.id)
    .bind(&p.theme)
    .bind(&p.accent_color)
    .bind(&p.background_color)
    .bind(&p.background_style)
    .bind(&p.density)
    .bind(&p.default_view)
    .execute(&state.db)
    .await?;

    state.events.send("prefs.updated", me.id, &p);
    Ok(Json(p))
}

#[derive(Deserialize)]
struct ProfileBody {
    display_name: Option<String>,
    avatar_color: Option<String>,
}

async fn update_profile(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<ProfileBody>,
) -> AppResult<Json<Me>> {
    let display_name = match body.display_name {
        Some(v) => validate::name("display name", &v, 60)?,
        None => me.display_name.clone(),
    };
    let avatar_color = match body.avatar_color {
        Some(v) => validate::color(&v)?,
        None => me.avatar_color.clone(),
    };
    sqlx::query("UPDATE users SET display_name = ?, avatar_color = ? WHERE id = ?")
        .bind(display_name)
        .bind(avatar_color)
        .bind(me.id)
        .execute(&state.db)
        .await?;

    let user = get_user(&state.db, me.id).await?;
    state.events.send("user.updated", me.id, &user);
    Ok(Json(me_for(&state.db, user).await?))
}
