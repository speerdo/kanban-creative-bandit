-- M3: per-user appearance settings. A user without a row gets the defaults in src/prefs.rs.
-- Colors are palette names ('blue') or '#rrggbb'. background_color NULL = the theme's canvas.
CREATE TABLE user_prefs (
    user_id          INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    theme            TEXT NOT NULL DEFAULT 'system' CHECK (theme IN ('system', 'light', 'dark')),
    accent_color     TEXT NOT NULL DEFAULT 'blue',
    background_color TEXT,
    background_style TEXT NOT NULL DEFAULT 'solid'
                     CHECK (background_style IN ('solid', 'gradient', 'subtle-pattern')),
    density          TEXT NOT NULL DEFAULT 'comfortable' CHECK (density IN ('comfortable', 'compact')),
    default_view     TEXT NOT NULL DEFAULT 'list' CHECK (default_view IN ('list', 'board')),
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
