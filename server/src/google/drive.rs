//! Drive: just enough to show a linked file's real name (`drive.metadata.readonly`).

use reqwest::Method;
use serde::Deserialize;
use sqlx::SqlitePool;

use super::{GResult, Google};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub name: String,
    pub mime_type: String,
}

impl Google {
    pub async fn drive_file(&self, db: &SqlitePool, user: i64, id: &str) -> GResult<DriveFile> {
        let url = self.drive_url(&["files", id])?;
        let query = [
            ("fields", "name,mimeType".to_string()),
            ("supportsAllDrives", "true".to_string()),
        ];
        let v = self
            .call(db, user, Method::GET, url, &query, None)
            .await?
            .unwrap_or_default();
        Ok(serde_json::from_value(v)
            .map_err(|e| anyhow::anyhow!("reading Google's answer: {e}"))?)
    }
}
