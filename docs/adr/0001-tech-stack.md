# ADR 0001: Tech stack

- **Status:** Proposed
- **Date:** 2026-10-08
- **Deciders:** Adam, Kat

## Context

We need a self-hosted kanban/list task tracker for two people. Requirements and constraints:

- Backend preferably in **Rust**.
- A polished, highly interactive browser UI: drag and drop, inline editing, theming, keyboard shortcuts.
  It must work in any modern browser on any OS, including phones.
- It runs on a low-power home server (Celeron N5095) **shared with a trading bot** that must not be
  disturbed. Small memory footprint, simple ops, no extra daemons.
- Two users, LAN only.

## Decision

| Concern | Choice |
|---|---|
| HTTP server | **Axum** (Tokio) |
| Database | **SQLite** via **SQLx** (compile-checked queries, built-in migrations), WAL mode |
| Frontend | **Svelte 5 + TypeScript**, built with **Vite** as a static SPA |
| Packaging | Frontend `dist/` embedded into the binary with **rust-embed**, so it ships as one file |
| Live sync | **Server-Sent Events** (`axum::response::sse`) fed by a `tokio::sync::broadcast` channel |
| Auth | argon2id password hashes + opaque session cookie |
| Drag and drop | **SortableJS** (chosen in M2: no framework coupling, mature touch support with press-to-drag, so swipes still scroll on phones) |
| Process mgmt | **systemd** unit with CPU/IO/memory limits. No Docker. |

## Alternatives considered

### Frontend: Leptos / Dioxus (all-Rust, WASM)
- **Pro:** one language end to end, with types shared between client and server.
- **Con:** Mature drag-and-drop, color pickers, date pickers and markdown editors are thin or DIY.
  WASM compile times on this CPU are long, and iteration on UI polish (our top priority) is slower.
- **Verdict:** Not now. The API is plain JSON, so a later rewrite of the UI in Leptos stays possible.

### Frontend: HTMX + server templates (Askama)
- **Pro:** minimal JS and a very small footprint.
- **Con:** Rich interactions (optimistic drag and drop across columns, slide-over detail panel, keyboard
  navigation, live theming) become awkward. Ends up with a lot of custom JS anyway.

### Frontend: React
- Also a fine choice. Svelte was picked for smaller bundles, less boilerplate, and built-in transitions
  that suit a polished board UI. This is a preference, not a hard requirement.

### Database: existing PostgreSQL 16/17 on the box
- **Pro:** already running.
- **Con:** couples the kanban app to the trading bot's database server (upgrades, restarts, resource
  contention). Backups are more involved. Two users don't need it. SQLx keeps a later migration possible.

### Docker
- Not installed, and it would add a daemon and overhead to a box we're protecting. A static binary plus
  systemd gives the same isolation we need via unit hardening options.

## Consequences

- Two toolchains (Cargo + npm). Node is already installed, and Rust needs `rustup`.
- Dev loop: `vite dev` (hot reload, proxies `/api` to `cargo run` on :8080). Production is a single
  binary.
- Release builds must use `nice`/`-j 2` to protect the trading bot (see [hosting.md](../hosting.md)).
- Types are duplicated between Rust and TS. Mitigate by generating TS types from Rust with
  `ts-rs` (decide in M1).
