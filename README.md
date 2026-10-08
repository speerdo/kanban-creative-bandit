# kanban-creative-bandit

A small, self-hosted, Asana-style task tracker for a two-person team (Adam & Kat).
List and board views, fast status changes, strong color coding, light/dark themes,
and per-user custom colors. Runs as a single Rust binary on our home server and is
used from any browser on the home network.

> **Status:** Blueprint stage. No application code yet. Start with [`docs/`](docs/README.md).

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

## Planned layout

```
.
├── server/        # Rust crate (Axum API, SQLite, embeds the built frontend)
│   └── migrations/
├── web/           # Svelte + Vite frontend
├── deploy/        # systemd unit, backup timer, install script
└── docs/
```
