-- M1 core schema: users, sessions, projects, per-project statuses, tasks.
-- Timestamps are UTC ISO-8601 text. `position` columns hold fractional-index keys
-- (see src/position.rs) so reordering touches a single row.

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name  TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    avatar_color  TEXT NOT NULL DEFAULT 'violet',
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY NOT NULL,   -- sha256 of the cookie token; the token itself is never stored
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    expires_at TEXT NOT NULL,
    user_agent TEXT
);
CREATE INDEX sessions_user ON sessions(user_id);

CREATE TABLE projects (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT 'blue',
    description TEXT NOT NULL DEFAULT '',
    archived    INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    position    TEXT NOT NULL,
    created_by  INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE statuses (
    id         INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,
    category   TEXT NOT NULL CHECK (category IN ('todo', 'in_progress', 'done')),
    position   TEXT NOT NULL
);
CREATE INDEX statuses_project ON statuses(project_id, position);

CREATE TABLE tasks (
    id             INTEGER PRIMARY KEY,
    project_id     INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    status_id      INTEGER NOT NULL REFERENCES statuses(id) ON DELETE RESTRICT,
    parent_task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
    title          TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    assignee_id    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    priority       TEXT NOT NULL DEFAULT 'none'
                   CHECK (priority IN ('none', 'low', 'medium', 'high', 'urgent')),
    due_date       TEXT,               -- YYYY-MM-DD
    start_date     TEXT,               -- YYYY-MM-DD
    position       TEXT NOT NULL,
    completed_at   TEXT,
    created_by     INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX tasks_project_status ON tasks(project_id, status_id, position);
CREATE INDEX tasks_assignee ON tasks(assignee_id);
