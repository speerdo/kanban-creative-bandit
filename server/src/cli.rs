//! Admin commands. There is no sign-up page; accounts are made on the server:
//!
//! ```text
//! kanban user add <username> [display name]
//! kanban user passwd <username>
//! kanban user list
//! kanban backup <file>  |  kanban backup --dir <dir> [--keep N]
//! kanban restore <backup file>       (with the service stopped)
//! kanban check                       (integrity check of the live database)
//! ```
//!
//! In production run them as the service user so they use the live database:
//! `sudo -u kanban KANBAN_DB_PATH=/var/lib/kanban/kanban.db kanban user add adam "Adam"`.

use std::path::Path;

use anyhow::{Context, bail};
use sqlx::SqlitePool;

use crate::{auth, backup, config::Config, validate};

const USAGE: &str = "usage:
  kanban                                 run the server
  kanban user add <username> [display name]
  kanban user passwd <username>
  kanban user list
  kanban backup <file>                   copy the live database (safe while running)
  kanban backup --dir <dir> [--keep N]   timestamped copy, keeping the newest N (default 14)
  kanban restore <backup file>           replace the database (stop the service first)
  kanban check                           integrity check of the database";

pub async fn run(db: &SqlitePool, config: &Config, args: &[String]) -> anyhow::Result<()> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["backup", "--dir", dir, rest @ ..] => {
            let keep = match rest {
                [] => 14,
                ["--keep", n] => n.parse().context("--keep takes a number")?,
                _ => bail!("{USAGE}"),
            };
            let path = backup::backup_into_dir(db, Path::new(dir), keep).await?;
            println!("backed up to {}", path.display());
        }
        ["backup", file] => {
            let bytes = backup::backup_to(db, Path::new(file)).await?;
            println!("backed up to {file} ({} KiB)", bytes / 1024);
        }
        ["check"] => {
            backup::check(&config.db_path).await?;
            println!("{}: ok", config.db_path.display());
        }
        ["user", "add", username, rest @ ..] => {
            let username = validate::username(username)?;
            let display = if rest.is_empty() {
                username.clone()
            } else {
                validate::name("display name", &rest.join(" "), 60)?
            };
            let hash = auth::hash_password(&read_new_password()?)?;
            sqlx::query(
                "INSERT INTO users (username, display_name, password_hash) VALUES (?, ?, ?)",
            )
            .bind(&username)
            .bind(&display)
            .bind(hash)
            .execute(db)
            .await
            .with_context(|| format!("adding user {username} (does it already exist?)"))?;
            println!("added {username} ({display})");
        }
        ["user", "passwd", username] => {
            let hash = auth::hash_password(&read_new_password()?)?;
            let mut tx = db.begin().await?;
            let done = sqlx::query("UPDATE users SET password_hash = ? WHERE username = ?")
                .bind(hash)
                .bind(username)
                .execute(&mut *tx)
                .await?;
            if done.rows_affected() == 0 {
                bail!("no user named {username}");
            }
            // A new password signs out every existing session.
            sqlx::query(
                "DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE username = ?)",
            )
            .bind(username)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            println!("password changed for {username}; their sessions were signed out");
        }
        ["user", "list"] => {
            let users: Vec<(String, String, String)> =
                sqlx::query_as("SELECT username, display_name, created_at FROM users ORDER BY id")
                    .fetch_all(db)
                    .await?;
            for (u, d, c) in users {
                println!("{u:<16} {d:<24} since {c}");
            }
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
    Ok(())
}

/// Prompts twice on a terminal; reads one line from stdin when piped
/// (`printf '%s\n' "$PW" | kanban user add kat`).
fn read_new_password() -> anyhow::Result<String> {
    use std::io::{BufRead, IsTerminal};

    let password = if std::io::stdin().is_terminal() {
        let first = rpassword::prompt_password("Password: ")?;
        let again = rpassword::prompt_password("Again: ")?;
        if first != again {
            bail!("passwords didn't match");
        }
        first
    } else {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        line.trim_end_matches(['\r', '\n']).to_string()
    };
    if password.chars().count() < 8 {
        bail!("use at least 8 characters");
    }
    Ok(password)
}

/// `kanban restore <file>`: refuses while the server is running (it would keep the old file open).
pub async fn restore(config: &Config, args: &[String]) -> anyhow::Result<()> {
    let [file] = args else {
        bail!("usage: kanban restore <backup file>")
    };
    if std::net::TcpListener::bind(config.bind).is_err() {
        bail!(
            "something is listening on {}: stop the server first (sudo systemctl stop kanban)",
            config.bind
        );
    }
    let kept = backup::restore(&config.db_path, Path::new(file)).await?;
    println!(
        "restored {file} into {}\nthe previous database is kept at {}\nstart the server again (sudo systemctl start kanban)",
        config.db_path.display(),
        kept.display()
    );
    Ok(())
}
