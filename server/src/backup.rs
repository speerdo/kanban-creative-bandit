//! Backups and restore, built into the binary so the server needs no `sqlite3` CLI.
//!
//! A backup is a `VACUUM INTO` copy: consistent and compact, taken while the server keeps
//! running. The nightly timer (deploy/kanban-backup.timer) keeps the newest 14.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use sqlx::{
    ConnectOptions, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};

/// Writes a consistent copy of the live database to `dest` (replacing it atomically).
pub async fn backup_to(db: &SqlitePool, dest: &Path) -> anyhow::Result<u64> {
    // VACUUM INTO refuses to overwrite, and a half-written file must never look like a backup.
    let partial = dest.with_extension("partial");
    let _ = std::fs::remove_file(&partial);
    let target = partial
        .to_str()
        .context("backup path must be valid UTF-8")?;
    sqlx::query("VACUUM INTO ?")
        .bind(target)
        .execute(db)
        .await
        .with_context(|| format!("writing {}", partial.display()))?;
    std::fs::rename(&partial, dest)
        .with_context(|| format!("moving backup to {}", dest.display()))?;
    Ok(std::fs::metadata(dest)?.len())
}

/// A timestamped backup in `dir`, then deletes all but the newest `keep`.
pub async fn backup_into_dir(db: &SqlitePool, dir: &Path, keep: usize) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let stamp: String = sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H%M%SZ', 'now')")
        .fetch_one(db)
        .await?;
    let dest = dir.join(format!("kanban-{stamp}.db"));
    backup_to(db, &dest).await?;

    let mut backups = list(dir)?;
    backups.sort(); // the timestamp sorts chronologically
    let excess = backups.len().saturating_sub(keep.max(1));
    for old in &backups[..excess] {
        std::fs::remove_file(old)
            .with_context(|| format!("removing old backup {}", old.display()))?;
    }
    Ok(dest)
}

/// Backups in `dir` (files named `kanban-<timestamp>.db`).
pub fn list(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = vec![];
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("kanban-") && name.ends_with(".db") {
            out.push(path);
        }
    }
    Ok(out)
}

/// `PRAGMA integrity_check` on any database file, opened read-only.
pub async fn check(path: &Path) -> anyhow::Result<()> {
    let mut conn: SqliteConnection = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .connect()
        .await
        .with_context(|| format!("opening {}", path.display()))?;
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut conn)
        .await
        .with_context(|| format!("{} doesn't look like a kanban database", path.display()))?;
    if result != "ok" {
        bail!("{} is damaged: {result}", path.display());
    }
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('users', 'tasks')",
    )
    .fetch_one(&mut conn)
    .await?;
    if tables != 2 {
        bail!("{} isn't a kanban database", path.display());
    }
    Ok(())
}

/// Replaces the database at `db_path` with `backup`. The server must be stopped. The current
/// database is kept next to it as `<name>.before-restore` so a restore can be undone.
pub async fn restore(db_path: &Path, backup: &Path) -> anyhow::Result<PathBuf> {
    check(backup).await?;
    let keep = db_path.with_extension("db.before-restore");
    if db_path.exists() {
        // Fold any WAL contents in first, so the kept copy is complete on its own.
        if let Ok(mut conn) = SqliteConnectOptions::new()
            .filename(db_path)
            .connect()
            .await
        {
            let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(&mut conn)
                .await;
        }
        std::fs::rename(db_path, &keep)
            .with_context(|| format!("moving aside {}", db_path.display()))?;
    }
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", db_path.display()));
        let _ = std::fs::remove_file(side);
    }
    std::fs::copy(backup, db_path)
        .with_context(|| format!("copying {} into place", backup.display()))?;
    Ok(keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn backup_prune_and_restore_round_trip() {
        let dir = std::env::temp_dir().join(format!("kanban-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = dir.join("live.db");
        let db = crate::db::connect(&db_path).await.unwrap();
        sqlx::query("INSERT INTO users (username, display_name, password_hash) VALUES ('adam', 'Adam', 'x')")
            .execute(&db)
            .await
            .unwrap();

        let backups = dir.join("backups");
        for i in 0..3 {
            // Distinct timestamps: pre-create older-looking names to exercise pruning.
            std::fs::create_dir_all(&backups).unwrap();
            std::fs::write(
                backups.join(format!("kanban-2000-01-0{}T000000Z.db", i + 1)),
                b"old",
            )
            .unwrap();
        }
        let newest = backup_into_dir(&db, &backups, 2).await.unwrap();
        let mut left = list(&backups).unwrap();
        left.sort();
        assert_eq!(left.len(), 2, "keeps the newest two");
        assert_eq!(left[1], newest);
        check(&newest).await.unwrap();
        assert!(
            check(&left[0]).await.is_err(),
            "a junk file fails the check"
        );

        // Change the live data, then restore the backup over it.
        sqlx::query("DELETE FROM users").execute(&db).await.unwrap();
        db.close().await;
        let kept = restore(&db_path, &newest).await.unwrap();
        assert!(kept.exists());

        let db = crate::db::connect(&db_path).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(n, 1, "the restored database has the user again");
        db.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
