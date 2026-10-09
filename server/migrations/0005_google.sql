-- M6: Google Calendar (docs/adr/0002-google-integration.md). Everything here is per person:
-- each of us connects our own Google account.

CREATE TABLE google_accounts (
    user_id           INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    google_sub        TEXT NOT NULL,          -- Google's stable account id
    email             TEXT NOT NULL,
    granted_scopes    TEXT NOT NULL,
    refresh_token_enc BLOB NOT NULL,          -- nonce || XChaCha20-Poly1305 ciphertext
    connected_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_sync_at      TEXT,
    last_push_at      TEXT,
    last_error        TEXT
);

-- The calendars we use: exactly one 'tasks' calendar (where Push to Google writes) and any
-- number of read-only 'overlay' calendars drawn in the Calendar view.
CREATE TABLE google_calendars (
    id                 INTEGER PRIMARY KEY,
    user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    google_calendar_id TEXT NOT NULL,
    summary            TEXT NOT NULL,
    color              TEXT,
    role               TEXT NOT NULL CHECK (role IN ('tasks', 'overlay')),
    sync_token         TEXT,                  -- tasks calendar: incremental pulls
    synced_at          TEXT,
    UNIQUE (user_id, google_calendar_id)
);
CREATE UNIQUE INDEX google_calendars_one_tasks ON google_calendars(user_id) WHERE role = 'tasks';

-- A task pushed to someone's tasks calendar. No foreign key to tasks: when a task is deleted the
-- row stays behind, so the next push knows to delete its event in Google.
CREATE TABLE task_events (
    task_id            INTEGER NOT NULL,
    user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    google_calendar_id TEXT NOT NULL,
    google_event_id    TEXT NOT NULL,
    etag               TEXT,                  -- our last write; a pull that sees it again skips it
    pushed_summary     TEXT NOT NULL,         -- what Google has, to count changes waiting
    pushed_date        TEXT NOT NULL,
    synced_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (task_id, user_id)
);
CREATE UNIQUE INDEX task_events_event ON task_events(user_id, google_event_id);

-- Read-only copies of events from overlay calendars, refreshed on every pull.
CREATE TABLE calendar_events (
    calendar_id     INTEGER NOT NULL REFERENCES google_calendars(id) ON DELETE CASCADE,
    google_event_id TEXT NOT NULL,
    summary         TEXT NOT NULL,
    start_at        TEXT NOT NULL,            -- YYYY-MM-DD (all day) or RFC 3339
    end_at          TEXT NOT NULL,            -- exclusive, same format
    all_day         INTEGER NOT NULL CHECK (all_day IN (0, 1)),
    html_link       TEXT,
    location        TEXT,
    PRIMARY KEY (calendar_id, google_event_id)
);
CREATE INDEX calendar_events_start ON calendar_events(start_at);
