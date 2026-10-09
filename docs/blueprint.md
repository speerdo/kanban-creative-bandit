# Blueprint

## 1. Goal

An internal productivity tool for a team of two (Adam & Kat) with the parts of Asana we actually
use, and nothing else:

- **List view** and **board (kanban) view** of the same tasks
- **Fast capture and status changes**: adding a task or moving it should take one or two actions
- **Strong, consistent color coding** for projects, statuses, priorities and labels
- **Light and dark mode**, plus **per-user custom colors** (accent, background)
- **Self-hosted** on the home server, reachable from any browser and OS on the home network
- **Google integration**: a calendar view that automatically pulls in our **Google Calendar** events,
  with task due dates pushed to Google **on demand**, Gmail threads turned into tasks, and Drive links on tasks.
- **Shared lists** (groceries, household) on both phones, replacing what we use Keep for.
  See §9 and [ADR 0002](adr/0002-google-integration.md).

### Non-goals (for now)

- Public internet exposure, SSO, multi-tenant or org management
- Native mobile apps (the web UI must still work well on a phone browser)
- Gantt, timelines, time tracking and automations. These can come later if wanted.
- Integrations beyond Google (Slack, GitHub, …)

### Constraints

- The host also runs the **trading bot**, which must never be starved of CPU, RAM or disk I/O.
  See [hosting.md](hosting.md).
- Low-power CPU (Celeron N5095, 4 cores). Keep the runtime footprint small and the builds cheap.
- Two users. Optimize for simplicity over scale.

---

## 2. Architecture

```
 Browser (Adam)   Browser (Kat)        any OS, any modern browser
       │                │
       └──── HTTP ──────┘   LAN only: http://192.168.4.32:8080
                │
        ┌───────▼─────────────────────────────┐
        │  kanban (single Rust binary)        │
        │  ├─ Axum router                     │
        │  │   ├─ /api/*    JSON REST API     │
        │  │   ├─ /api/events  SSE stream     │
        │  │   └─ /*        embedded SPA      │
        │  ├─ auth (argon2 + session cookie)  │
        │  ├─ google pull worker (every 15 m) │──── HTTPS (outbound only) ──► Google Calendar,
        │  └─ SQLx ──► SQLite (WAL)           │                               Gmail, Drive APIs
        └─────────────────────────────────────┘
                │
        /var/lib/kanban/kanban.db  (+ nightly backups)
```

- **One process, one file.** The Svelte frontend is built to static assets and embedded into the
  binary with `rust-embed`. Deploying means copying one binary and restarting one service.
- **SQLite** rather than the Postgres instances already on the box, so the kanban app shares
  nothing with the trading bot's database and backups are a single file copy.
- **Live updates:** when either person changes something, the server broadcasts an event over
  Server-Sent Events. The other browser updates in place, with no refresh needed.
- **Google:** a background task pulls changes from Google every 15 minutes. Task dates go *to* Google
  only when someone presses **Push to Google**. Traffic to Google is outbound only, and nothing new is exposed
  to the internet.

### Repository layout

```
server/
  Cargo.toml
  migrations/            # sqlx migrations (plain SQL, numbered)
  src/
    main.rs              # config, router, startup
    db.rs                # pool, pragmas
    auth.rs              # login, sessions, extractor
    events.rs            # SSE broadcast hub
    routes/              # projects.rs, tasks.rs, statuses.rs, labels.rs, comments.rs, prefs.rs, calendar.rs
    google/              # oauth.rs, client.rs, calendar.rs, gmail.rs, drive.rs, sync.rs (M6–M7)
    import/keep.rs       # optional Google Takeout import (M7)
    models/              # row structs + DTOs
    assets.rs            # rust-embed of web/dist
web/
  package.json
  vite.config.ts         # dev proxy /api -> localhost:8080
  src/
    lib/api.ts           # typed fetch client
    lib/stores/          # tasks, projects, prefs (Svelte runes)
    lib/theme/           # tokens, color utilities
    lib/components/      # TaskCard, TaskRow, StatusPill, ColorPicker, QuickAdd, …
    routes/              # ListView, BoardView, MyTasks, CalendarView, TaskDetail, Settings
deploy/
  kanban.service         # systemd unit with resource limits
  kanban-backup.service / .timer
  install.sh
```

---

## 3. Data model

All ids are integers. Timestamps are UTC ISO-8601 text. Ordering uses **fractional index strings**
(`position`), so a drag-and-drop move updates a single row.

```
users
  id, username (unique), display_name, password_hash, avatar_color, created_at

user_prefs                       -- one row per user
  user_id PK→users, theme ('system'|'light'|'dark'),
  accent_color, background_color, background_style ('solid'|'gradient'|'subtle-pattern'),
  density ('comfortable'|'compact'), default_view ('list'|'board'), updated_at

projects
  id, name, color, icon, description, archived (bool), position, created_by, created_at

statuses                         -- board columns / list sections, per project
  id, project_id→projects, name, color, position,
  category ('todo'|'in_progress'|'done')   -- drives "completed" logic + My Tasks

tasks
  id, project_id→projects, status_id→statuses, parent_task_id→tasks (subtasks, nullable),
  title, description (markdown), assignee_id→users (nullable),
  priority ('none'|'low'|'medium'|'high'|'urgent'),
  due_date (date, nullable), start_date (nullable),
  position, completed_at (nullable), created_by, created_at, updated_at

labels
  id, name, color                -- global, shared across projects

task_labels
  task_id, label_id  (PK both)

comments
  id, task_id, author_id, body (markdown), created_at, edited_at

activity                         -- append-only audit trail shown on the task
  id, task_id, actor_id, kind ('created'|'status'|'assignee'|'due'|'priority'|'title'|…),
  from_value, to_value, created_at

sessions
  id (random 256-bit token, hashed), user_id, created_at, expires_at, user_agent
```

### Google integration tables (M6–M7)

```
google_accounts                  -- one per user who connected Google
  user_id PK→users, google_sub, email, granted_scopes,
  refresh_token_enc (blob, XChaCha20-Poly1305), connected_at, last_sync_at, last_error

google_calendars                 -- the user's calendars that we use
  id, user_id→users, google_calendar_id, summary, color,
  role ('tasks'|'overlay'),      -- exactly one 'tasks' calendar per user
  sync_token, synced_at

task_events                      -- task ↔ Google event, per user
  task_id→tasks, user_id→users, google_calendar_id, google_event_id, etag, synced_at
  PK (task_id, user_id)

calendar_events                  -- cached read-only events from overlay calendars
  calendar_id→google_calendars, google_event_id, summary, start, end, all_day, html_link, updated
  PK (calendar_id, google_event_id)

task_links                       -- external things a task points at
  id, task_id→tasks, kind ('gmail'|'drive'|'url'), external_id, url, title, mime_type,
  added_by, created_at

gmail_imports
  user_id, thread_id, task_id→tasks, imported_at   PK (user_id, thread_id)
```

`task_events.pushed_summary` / `pushed_date` (what Google has) vs the task's current title, completion
and due date tells which tasks have changes waiting for the next **Push to Google**.

### Shared lists (M7)

```
lists                            -- e.g. Groceries, Household
  id, name, color, icon, position, created_by, created_at

list_items
  id, list_id→lists, text, note, position,
  checked_at (nullable), checked_by→users, created_by, created_at
```

**Default statuses** seeded for every new project (editable):

| Name | Category | Color |
|---|---|---|
| Backlog | todo | slate |
| To Do | todo | blue |
| In Progress | in_progress | amber |
| Review | in_progress | violet |
| Done | done | green |

Moving a task into a `done`-category status sets `completed_at`. Moving it out clears it.

---

## 4. API (REST + SSE)

JSON over HTTP. Every route except `/api/auth/login` requires a session cookie
(`HttpOnly`, `SameSite=Strict`).

| Method | Path | Purpose |
|---|---|---|
| POST | `/api/auth/login` · `/api/auth/logout` | Session management |
| GET | `/api/me` | Current user + prefs |
| PUT | `/api/me/prefs` | Theme, colors, density, default view |
| GET/POST | `/api/projects` | List / create |
| PATCH/DELETE | `/api/projects/:id` | Rename, recolor, archive, reorder |
| GET/POST | `/api/projects/:id/statuses` | Columns / sections |
| PATCH/DELETE | `/api/statuses/:id` | Rename, recolor, reorder, delete (with task reassignment) |
| GET | `/api/tasks?project=&assignee=&status=&due_before=&q=` | Filtered list (powers List, Board and My Tasks) |
| POST | `/api/tasks` | Create (quick add) |
| GET/PATCH/DELETE | `/api/tasks/:id` | Detail / partial update / delete |
| POST | `/api/tasks/:id/move` | `{status_id, before_id?, after_id?}`: the single drag-and-drop endpoint |
| GET/POST | `/api/tasks/:id/comments` | Comments |
| GET | `/api/tasks/:id/activity` | History |
| GET/POST/PATCH/DELETE | `/api/labels[/:id]` | Labels |
| GET | `/api/calendar?from=&to=` | Tasks with due dates in range + cached Google overlay events (Calendar view) |
| GET | `/api/integrations/google` | Connection status, email, granted features, last sync, last error |
| POST | `/api/integrations/google/start` | `{features}` → `{auth_url}` (PKCE + state kept server-side) |
| POST | `/api/integrations/google/finish` | `{redirected_url}`: exchanges the code, stores the encrypted refresh token |
| DELETE | `/api/integrations/google` | Revoke at Google and delete everything cached for this user |
| GET/PUT | `/api/integrations/google/calendars` | List your Google calendars / choose the tasks calendar (or "create Kanban") and overlays |
| PUT | `/api/integrations/google/gmail` | Turn Gmail import on or off, and pick the target project |
| POST | `/api/integrations/google/sync` | Pull from Google now |
| GET/POST | `/api/integrations/google/push` | Count of changes waiting / **Push to Google** |
| GET/POST | `/api/lists` | Shared lists |
| PATCH/DELETE | `/api/lists/:id` | Rename, recolor, reorder, delete |
| GET/POST | `/api/lists/:id/items` | Items / add (one per line when pasted) |
| PATCH/DELETE | `/api/list-items/:id` | Check/uncheck, edit, reorder |
| POST | `/api/lists/:id/clear-checked` | Remove checked items |
| GET/POST/DELETE | `/api/tasks/:id/links[/:link_id]` | Gmail, Drive and URL links on a task |
| POST | `/api/import/keep` | Upload a Takeout `.zip` → `{dry_run}` preview, or create tasks |
| GET | `/api/events` | SSE stream: `task.created`, `task.updated`, `task.moved`, `task.deleted`, `comment.created`, `project.*`, `status.*`, `calendar.synced`, `list.*`, `list_item.*` |

Updates are **optimistic** in the UI. If the server rejects a change, the UI rolls back and shows a toast.
Conflicts are last-write-wins per field. That's acceptable for two people.

---

## 5. UI / UX

### Layout

```
┌───────────────┬────────────────────────────────────────────────────────┐
│ ● My Tasks    │  Project Name ▾     [ List | Board ]   Filter  Sort  ⚙  │
│ ─ Projects ─  ├────────────────────────────────────────────────────────┤
│ ● Website     │                                                        │
│ ● Ops         │     List or Board view                                 │
│ ● Personal    │                                                        │
│ + New project │                                                        │
│               │                                     ┌─ Task detail ──┐ │
│ ⚙ Settings    │                                     │ slide-over pane│ │
└───────────────┴─────────────────────────────────────┴────────────────┴─┘
```

- **Sidebar:** My Tasks, a projects list with color dots (drag to reorder), and Settings. It collapses on narrow screens.
- **Task detail:** a slide-over panel (Asana-style), so the list or board stays visible behind it. Includes the
  description (markdown), subtasks, labels, assignee, due date, priority, comments and activity.

### List view

- Tasks are grouped into collapsible **sections by status**. Each section header carries its status color.
- Columns: ☐ complete · title · assignee avatar · due date · priority · labels.
- The **inline status pill** opens a color-coded dropdown, and you can also drag rows between sections.
- An inline "+ Add task" row at the bottom of each section. Press Enter to create the task and start the next one.

### Board view

- One column per status, with a colored header bar and a task count.
- Cards show the title, a priority stripe on the left edge, label chips, the assignee avatar, the due date (red if overdue) and the subtask count.
- Drag and drop between and within columns using the `/move` endpoint. Each column has a "+" for quick add.
- Columns scroll horizontally on small screens. Touch drag works on phones and tablets.

### My Tasks

Everything assigned to me across all projects, grouped by **Overdue · Today · This week · Later · No date**.

### Calendar (M6)

- **Month** and **week** layouts. Tasks sit on their due date as chips in the project color, with the status
  pill and assignee avatar. You can drag a task to another day to reschedule it, or click it to open the detail panel.
- Your chosen **Google calendars** are drawn beside the tasks, muted and read-only, in each calendar's
  Google color. Clicking an event opens it in Google Calendar.
- Filters: me / Kat / everyone, and per-project toggles. Next to the last-pulled time there's a
  "Sync now" button (pulls from Google) and **Push to Google (n)**, which is enabled when tasks have
  changes waiting.
- Click an empty day to quick-add a task due that day.

### Lists (M7)

- A **Lists** entry in the sidebar, with one page per list (Groceries, Household, …). It's built for a phone in one hand.
- A big add box at the top that stays focused after Enter. Pasting several lines adds several items.
- Tap an item to check it off. Checked items sink into a collapsed "Checked (n)" group, and **Clear checked**
  removes them. Unchecking brings an item back, so regular groceries can be reused.
- Changes appear live on the other phone (SSE). The app can be added to the home screen (web app manifest).

### Settings → Google (M6–M7)

- **Google:** shows the connect/disconnect state and the account email. Lets you pick the tasks calendar and the
  overlay calendars, and has toggles for Gmail import and Drive titles. Shows the last sync and any error in plain words
  (e.g. "Google access was revoked. Reconnect.").
- **Import (optional):** upload a Google Keep Takeout file to seed Lists, preview what will be created, then confirm.

### Fast capture and status changes

| Action | How |
|---|---|
| Quick add from anywhere | Press `Q` (or the + button) to open a one-line input. Typing `Fix login !high @kat #bug fri` parses the priority, assignee, label and due date. |
| Change status | Click the status pill, or press `1`–`9` while a task is focused or hovered |
| Complete | Click the checkbox, or press `Ctrl/⌘+Enter` |
| Move on the board | Drag, or press `[` / `]` to move one column left or right |
| Navigate | `J`/`K` to move down/up, `Enter` to open, `Esc` to close, `/` to search, `?` for the shortcut sheet |

---

## 6. Color system and theming

The whole UI is driven by **CSS custom properties** (design tokens) defined in `:root` and
overridden per theme. Components never hard-code colors.

### Semantic layers

| Layer | Source | Used for |
|---|---|---|
| Project color | `projects.color` | Sidebar dot, page header accent, card top border in My Tasks |
| Status color | `statuses.color` | Section headers, board column bars, status pills |
| Priority color | Fixed scale | Card left stripe, priority badge: urgent=red, high=orange, medium=yellow, low=blue, none=transparent |
| Label color | `labels.color` | Chips |
| Accent color | `user_prefs.accent_color` | Buttons, focus rings, selection, links |
| Background | `user_prefs.background_*` | App canvas behind the panels |

### Palette

A curated palette of about 12 named hues (slate, red, orange, amber, yellow, lime, green, teal, cyan, blue,
violet, pink). Each hue is defined in **OKLCH** with a light-mode and a dark-mode variant, so the same
"blue" looks right in both themes. The color pickers offer this palette first, then "Custom…" for any hex value.

### Readability guarantees

- The text color on any colored surface (pills, chips, column headers) is **computed** from the
  background's lightness, never chosen by hand. The target is WCAG AA contrast (4.5:1 for text).
- Custom background colors are applied to the **canvas only**. Panels and cards keep theme surface
  colors, so a bold background never makes task text unreadable.
- Status is never conveyed by color alone. Pills always include the status name, and priority has an icon too.

### Theme modes

- `system` (default) follows `prefers-color-scheme`. `light` and `dark` are explicit overrides.
- Preferences are stored per user on the server, so they follow you to any device. They're also cached
  in `localStorage` to avoid a flash of the wrong theme on load.
- Settings page: theme toggle, accent picker, background picker (solid / gradient / subtle pattern),
  density, and a live preview.

---

## 7. Security (LAN-appropriate)

- Passwords hashed with **argon2id**. Accounts are created by a CLI command
  (`kanban user add <name>`), and there is no public sign-up page.
- Session tokens are random. Only a hash is stored. Cookies are `HttpOnly` and `SameSite=Strict`, with a 30-day sliding expiry.
- Bound to the LAN interface and port, with the firewall restricted to the home subnet (see
  [hosting.md](hosting.md)). It is not exposed to the internet.
- All SQL goes through SQLx bound parameters. Markdown is rendered in the client with HTML sanitized.
- If remote access is ever wanted, put it behind Tailscale (or Caddy with TLS) rather than port-forwarding.
- **Google tokens:** refresh tokens are encrypted at rest with a key passed in via systemd `LoadCredential=`
  and kept outside the database and its backups. Access tokens live only in memory. Scopes are the minimum
  per feature, and Gmail and Drive are opt-in. Disconnecting revokes the token at Google. Each user's
  Google data is visible only to that user, apart from the tasks it creates.

---

## 8. Roadmap

| Milestone | Scope | Done when |
|---|---|---|
| **M0: Scaffold** | Cargo crate, Vite/Svelte app, dev proxy, migrations runner, health endpoint, embedded assets, systemd unit | `kanban` binary serves "hello" to both laptops on the LAN |
| **M1: Core tasks** | Auth, projects, statuses, task CRUD, **List view**, status pill, quick add | We can run real work in list view |
| **M2: Board** | **Board view** with drag and drop, the `/move` endpoint, SSE live sync | Both of us see each other's moves live |
| **M3: Color and themes** | Token system, light/dark/system, palette, accent + background pickers, per-user prefs | Settings page fully working on both devices |
| **M4: Depth** | Task detail panel, subtasks, labels, comments, activity, My Tasks, filters/search, keyboard shortcuts | Daily-driver quality |
| **M5: Ops polish** | Nightly backups + restore script, `install.sh`, update procedure, basic metrics/log rotation | Hands-off running for weeks |
| **M6: Calendar + Google Calendar** | Google connect (paste-back OAuth, encrypted tokens), Calendar view (month/week, drag to reschedule), 15-minute pull of chosen calendars and the "Kanban" calendar, **Push to Google** button for task dates | Google edits show up in the app on their own; one button puts our tasks on the Kanban calendar |
| **M7: Gmail, Drive, Lists** | Task links, Gmail `Kanban` label → task, Drive link chips (+ optional titles), shared **Lists** with live sync and home-screen install, optional Keep Takeout import. Needs Tailscale for use away from home. | Email-to-task works from the phone; groceries live in the app instead of Keep |

Later ideas: recurring tasks, due-date reminders (browser notifications), optional due *times* (timed
calendar events instead of all-day), attachments, templates, import from Asana CSV, Tailscale HTTPS
(unlocks Calendar push and the Drive Picker).

---

## 9. Google integration (summary)

The full rationale is in [ADR 0002](adr/0002-google-integration.md), and setup steps are in [google-setup.md](google-setup.md).

| Google app | What we do | How |
|---|---|---|
| **Calendar** | Your Google calendars show in the app's Calendar view, refreshed automatically every 15 minutes. **Push to Google** puts your task due dates on your "Kanban" calendar as all-day events. Moving or deleting them in Google updates the task. | Calendar API, incremental `syncToken` pulls, pushes only on request |
| **Gmail** | Label a thread `Kanban` and it becomes a task with a link back to the thread | Gmail API (`gmail.modify`), opt-in |
| **Drive** | Docs, Sheets, Slides and Drive links on a task render as typed chips, with real titles if enabled | URL parsing, plus optional `drive.metadata.readonly` |
| **Keep** | Replaced by in-app shared **Lists** (groceries, household). An optional one-time import from Keep is available. | Keep has no API for personal accounts, so the import uses a Google Takeout file. |

**Why the paste-back step:** Google only redirects to HTTPS domains or `127.0.0.1`, and the app is plain HTTP
on a LAN IP. A Desktop-type OAuth client redirects to `127.0.0.1`. You copy that URL back into the app once,
and from then on the server keeps itself authorized.
