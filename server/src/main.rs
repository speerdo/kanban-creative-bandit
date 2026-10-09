mod api;
mod assets;
mod auth;
mod backup;
mod cli;
mod config;
mod db;
mod error;
mod events;
mod position;
mod prefs;
mod routes;
mod validate;

#[cfg(test)]
mod tests;

use std::time::Duration;

use axum::Router;
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub events: events::Hub,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .nest("/api", api::router())
        .fallback(assets::serve)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();

    let config = config::Config::from_env()?;
    started();

    // `kanban user …` and friends: run the command and exit instead of serving. Restore runs
    // before the database is opened, since it replaces the file.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("restore") {
        return cli::restore(&config, &args[1..]).await;
    }
    let db = db::connect(&config.db_path).await?;
    if !args.is_empty() {
        return cli::run(&db, &config, &args).await;
    }
    tracing::info!(path = %config.db_path.display(), "database ready");

    tokio::spawn(purge_sessions_daily(db.clone()));

    let (events, stop_events) = events::Hub::new();
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!("listening on http://{}", config.bind);
    axum::serve(
        listener,
        app(AppState {
            db: db.clone(),
            events,
        }),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        // End the open SSE streams, or shutdown would wait on every browser tab.
        let _ = stop_events.send(true);
    })
    .await?;

    db.close().await;
    tracing::info!("shut down cleanly");
    Ok(())
}

/// When the process started (for `/api/health`).
pub fn started() -> std::time::Instant {
    static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *STARTED.get_or_init(std::time::Instant::now)
}

async fn purge_sessions_daily(db: SqlitePool) {
    let mut tick = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
    loop {
        tick.tick().await;
        match auth::purge_expired_sessions(&db).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(removed = n, "purged expired sessions"),
            Err(e) => tracing::warn!("purging sessions: {e}"),
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
