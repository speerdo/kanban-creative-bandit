# Docs

| Doc | What it covers |
|---|---|
| [blueprint.md](blueprint.md) | Product scope, architecture, data model, API, UI/UX, theming, roadmap |
| [google-setup.md](google-setup.md) | One-time Google Cloud project + OAuth client setup, and how each of us connects |
| [hosting.md](hosting.md) | How it runs on the home server next to the trading bot: ports, firewall, resource limits, backups |
| [adr/](adr/) | Architecture Decision Records. One file per significant decision, numbered, never rewritten (supersede instead) |

## Open questions

Decisions we still need from Adam & Kat before or during Milestone 1:

1. ~~**Frontend:**~~ Svelte + TS, as built in M0/M1. See [ADR 0001](adr/0001-tech-stack.md).
2. **Statuses:** One shared status set for every project, or per-project custom columns? The blueprint assumes per-project, seeded with defaults.
3. **Login:** Is a password login wanted on a trusted LAN, or is a "who are you?" picker enough? M1 ships passwords (accounts via `kanban user add`).
4. **Remote access:** LAN only for now. Do we want phone access away from home later (e.g. Tailscale)?
5. **Hostname:** Keep `http://192.168.4.32:8080`, or set up a friendlier name like `kanban.local`?
6. ~~**Google calendar target**~~ (decided 2026-10-09): a separate "Kanban" calendar. Google → app is pulled automatically. App → Google happens only via a **Push to Google** button.
7. ~~**Keep**~~ (decided 2026-10-09): what we use it for is shared grocery and household lists. These become in-app **Lists** (M7). A Keep import is a nice-to-have.
8. ~~**Gmail**~~ (decided 2026-10-09): label a thread `Kanban` → it becomes a task.
9. **Remote access is now needed** for Lists to be useful at the store. Set up Tailscale on the server and both phones before M7? (This also answers question 4.)
