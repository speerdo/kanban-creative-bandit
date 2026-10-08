use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;

pub struct Config {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind = std::env::var("KANBAN_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .context("KANBAN_BIND must be an address like 0.0.0.0:8080")?;
        let db_path = std::env::var("KANBAN_DB_PATH")
            .unwrap_or_else(|_| "data/kanban.db".into())
            .into();
        Ok(Self { bind, db_path })
    }
}
