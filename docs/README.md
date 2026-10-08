# Docs

| Doc | What it covers |
|---|---|
| [blueprint.md](blueprint.md) | Product scope, architecture, data model, API, UI/UX, theming, roadmap |
| [hosting.md](hosting.md) | How it runs on the home server next to the trading bot: ports, firewall, resource limits, backups |
| [adr/](adr/) | Architecture Decision Records. One file per significant decision, numbered, never rewritten (supersede instead) |

## Open questions

Decisions we still need from Adam & Kat before or during Milestone 1:

1. **Frontend:** Svelte + TS (recommended) or all-Rust Leptos? See [ADR 0001](adr/0001-tech-stack.md).
2. **Statuses:** One shared status set for every project, or per-project custom columns? The blueprint assumes per-project, seeded with defaults.
3. **Login:** Is a password login wanted on a trusted LAN, or is a "who are you?" picker enough? The blueprint assumes passwords.
4. **Remote access:** LAN only for now. Do we want phone access away from home later (e.g. Tailscale)?
5. **Hostname:** Keep `http://192.168.4.32:8080`, or set up a friendlier name like `kanban.local`?
