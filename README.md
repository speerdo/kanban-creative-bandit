# kanban-creative-bandit

A small, self-hosted, Asana-style task tracker for a two-person team (Adam & Kat).
List and board views, fast status changes, strong color coding, light/dark themes,
and per-user custom colors, plus a calendar that syncs with Google Calendar and
Gmail/Drive/Keep hooks. Runs as a single Rust binary on our home server and is
used from any browser on the home network.

> **Status:** M4 (depth): list and board views with drag and drop and live sync; a task detail panel
> with markdown description, subtasks, labels, comments and history; My Tasks; filters and search;
> keyboard shortcuts (press `?`); per-user themes. Next up: M5 (backups and ops polish), then Google
> Calendar (M6) and Gmail, Drive and shared Lists (M7).

## Quick start

Requires Rust (`rustup`) and Node 20+. Run `make help` to list every target.

**Develop:** run these in two terminals:

```bash
make dev-server   # Rust API on :8080, creates data/kanban.db
make dev-web      # Vite on http://localhost:5173 with hot reload, proxies /api to :8080
```

Create accounts (there's no sign-up page). In dev this uses `data/kanban.db`:

```bash
cargo run -- user add adam "Adam"     # prompts for a password
cargo run -- user add kat "Kat"
```

**Deploy on the home server:**

```bash
make install      # builds frontend + release binary, then runs sudo deploy/install.sh
```

The installer creates the `kanban` system user, installs the systemd unit (resource-capped so the
trading bot always wins) and opens port 8080 to the LAN only if `ufw` is active. Then open
`http://192.168.4.32:8080` from any device on the home network.

Then create the accounts against the live database:

```bash
sudo -u kanban KANBAN_DB_PATH=/var/lib/kanban/kanban.db /usr/local/bin/kanban user add adam "Adam"
```

Upgrade: `git pull && make install`. Logs: `journalctl -u kanban -f`.

## At a glance

| | |
|---|---|
| Backend | Rust ([Axum](https://github.com/tokio-rs/axum) + [SQLx](https://github.com/launchbadge/sqlx)) |
| Database | SQLite (single file, WAL mode) |
| Frontend | Svelte 5 + TypeScript (Vite), compiled and embedded into the Rust binary |
| Live updates | Server-Sent Events |
| Hosting | systemd service on the home server, LAN only, port `8080` |

## Docs

- [Blueprint](docs/blueprint.md): what we're building and how
- [Hosting on the home server](docs/hosting.md): running alongside the trading bot
- [ADR 0001: Tech stack](docs/adr/0001-tech-stack.md)
- [ADR 0002: Google integration](docs/adr/0002-google-integration.md) and [Google setup](docs/google-setup.md)

## Planned layout

```
.
├── server/        # Rust crate (Axum API, SQLite, embeds the built frontend)
│   └── migrations/
├── web/           # Svelte + Vite frontend
├── deploy/        # systemd unit, backup timer, install script
└── docs/
```
