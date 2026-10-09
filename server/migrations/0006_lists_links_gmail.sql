-- M7: shared lists (groceries, household), links on tasks (Gmail, Drive, any URL), and the
-- Gmail "Kanban" label import.

CREATE TABLE lists (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL DEFAULT 'green',
    position   TEXT NOT NULL,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE list_items (
    id         INTEGER PRIMARY KEY,
    list_id    INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    note       TEXT NOT NULL DEFAULT '',
    position   TEXT NOT NULL,
    checked_at TEXT,
    checked_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX list_items_list ON list_items(list_id, position);

CREATE TABLE task_links (
    id          INTEGER PRIMARY KEY,
    task_id     INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    -- gmail | doc | sheet | slides | form | drive (file or folder) | url
    kind        TEXT NOT NULL,
    external_id TEXT,                    -- Gmail thread id or Drive file id
    url         TEXT NOT NULL,
    title       TEXT,
    mime_type   TEXT,
    added_by    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX task_links_task ON task_links(task_id);

CREATE TABLE gmail_imports (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    thread_id   TEXT NOT NULL,
    task_id     INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, thread_id)
);

-- Gmail import is opt-in per person; tasks land in the chosen project.
ALTER TABLE google_accounts ADD COLUMN gmail_project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL;
ALTER TABLE google_accounts ADD COLUMN gmail_enabled INTEGER NOT NULL DEFAULT 0;
