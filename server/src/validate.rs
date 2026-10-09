//! Input checks shared by the route handlers.

use serde::{Deserialize, Deserializer};

use crate::error::{AppError, AppResult};

/// Named palette hues. The frontend maps each to light/dark OKLCH values.
pub const COLORS: &[&str] = &[
    "slate", "red", "orange", "amber", "yellow", "lime", "green", "teal", "cyan", "blue", "violet",
    "pink",
];

pub const PRIORITIES: &[&str] = &["none", "low", "medium", "high", "urgent"];
pub const CATEGORIES: &[&str] = &["todo", "in_progress", "done"];

pub fn name(field: &str, value: &str, max: usize) -> AppResult<String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(AppError::bad(format!("{field} can't be empty")));
    }
    if v.chars().count() > max {
        return Err(AppError::bad(format!("{field} is too long (max {max})")));
    }
    Ok(v.to_string())
}

/// Lowercase letters, digits, `-` and `_`; used to sign in and in `@mentions` for quick add.
pub fn username(value: &str) -> AppResult<String> {
    let v = value.trim().to_lowercase();
    let ok = (2..=32).contains(&v.len())
        && v.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if ok {
        Ok(v)
    } else {
        Err(AppError::bad(
            "usernames are 2-32 lowercase letters, digits, '-' or '_'",
        ))
    }
}

pub fn one_of(field: &str, value: &str, allowed: &[&str]) -> AppResult<String> {
    if allowed.contains(&value) {
        Ok(value.to_string())
    } else {
        Err(AppError::bad(format!(
            "{field} must be one of: {}",
            allowed.join(", ")
        )))
    }
}

/// A palette name, or any `#rrggbb` (the "Custom…" option). Hex is stored lowercase.
pub fn color(value: &str) -> AppResult<String> {
    let v = value.trim();
    let hex = v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        Ok(v.to_ascii_lowercase())
    } else if COLORS.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(AppError::bad(format!(
            "color must be #rrggbb or one of: {}",
            COLORS.join(", ")
        )))
    }
}

/// `YYYY-MM-DD` with a plausible month and day.
pub fn date(value: &str) -> AppResult<String> {
    let b = value.as_bytes();
    let ok = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        && (1..=12).contains(&value[5..7].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&value[8..10].parse::<u32>().unwrap_or(0));
    if ok {
        Ok(value.to_string())
    } else {
        Err(AppError::bad("dates must look like 2026-10-31"))
    }
}

/// For PATCH bodies: distinguishes a missing field (`None`) from an explicit `null`
/// (`Some(None)`, i.e. "clear this value"). Use with `#[serde(default)]`.
pub fn nullable<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}
