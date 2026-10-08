# Hosting on the home server

The kanban app shares a machine with the **trading bot** (`tradebrain`). The rule is:
**the trading bot always wins.** Everything below follows from that.

## Host snapshot (2026-10-08)

| | |
|---|---|
| Hardware | Intel Celeron N5095 (4 cores, 2.0 GHz), 15 GiB RAM, 210 GB disk (~166 GB free) |
| OS | Ubuntu 24.04.5 LTS, systemd 255 |
| LAN address | `192.168.4.32` on `enp1s0` (`192.168.4.0/22`) |
| mDNS | `avahi-daemon` active → `adam-Jasper-Lake-Client-Platform.local` |
| Installed | Node 24, npm 11, PostgreSQL 16 & 17, ufw |
| Not installed | Rust toolchain, Docker |

### Ports already in use, so don't collide

| Port | Bind | Owner |
|---|---|---|
| 22 | all | sshd |
| 53 | localhost | systemd-resolved |
| 631 | localhost | CUPS |
| 5432 / 5433 | localhost | PostgreSQL 16 / 17 |
| **8000** | localhost | **tradebrain** (`python -m agent.main`) |
| 11434 | localhost | Ollama |

**Kanban will use `0.0.0.0:8080`.** It's the only service intentionally exposed to the LAN
besides SSH.

## Protecting the trading bot

The app is tiny when idle (an estimated ~10–30 MB RSS, near-zero CPU). The real risks are **compile
time** and **runaway bugs**, and both are capped below.

### Runtime limits (systemd)

`deploy/kanban.service` (to be written in M0) will include:

```ini
[Service]
User=kanban
Group=kanban
ExecStart=/usr/local/bin/kanban
Environment=KANBAN_BIND=0.0.0.0:8080
Environment=KANBAN_DATABASE_URL=sqlite:///var/lib/kanban/kanban.db?mode=rwc
StateDirectory=kanban
Restart=on-failure

# Yield to the trading bot under contention
Nice=10
CPUWeight=20
IOWeight=20
MemoryHigh=192M
MemoryMax=256M
TasksMax=64

# Hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/var/lib/kanban
```

`CPUWeight`/`IOWeight` only matter when the CPU or disk is contended. When the bot is idle, the kanban app can
use spare capacity freely. When the bot is busy, it gets ~5× the share.

### Builds

`cargo build --release` on a 4-core Celeron can saturate every core for several minutes. Always build
with lower priority and fewer jobs:

```bash
nice -n 19 ionice -c3 cargo build --release -j 2
```

This will be wrapped in a `make release` / `just release` target in M0. The frontend build
(`vite build`) is quick and light.

### Isolation

- A separate system user (`kanban`). It can't read `~/Projects/tradebrain` or its `venv`.
- Its own SQLite file under `/var/lib/kanban`, so it never touches the bot's Postgres instances.

## Network access

```bash
# Allow only the home subnet to reach the app (sudo required)
sudo ufw allow from 192.168.4.0/22 to any port 8080 proto tcp comment 'kanban LAN'
sudo ufw status verbose
```

Then from any laptop or phone on the home Wi-Fi:

- `http://192.168.4.32:8080`, or
- `http://adam-Jasper-Lake-Client-Platform.local:8080` (mDNS works on macOS, iOS, Linux and Windows 10+.
  Android support varies.)

**Recommended:** create a **DHCP reservation** for `192.168.4.32` in the router, so the address never
changes. Optionally rename the host or add an Avahi alias to get a short name like `kanban.local`.

Do **not** port-forward 8080 on the router. If remote access is wanted later, use Tailscale.

## Backups

- Nightly systemd timer runs `sqlite3 /var/lib/kanban/kanban.db ".backup '/var/lib/kanban/backups/kanban-$(date +%F).db'"`
  (an online, consistent copy that's safe while the app runs), and keeps 14 days.
- Optionally rsync `backups/` to another machine or cloud drive.
- Restore: stop the service, copy the backup over `kanban.db`, then start the service.

(`sqlite3` CLI isn't installed yet: `sudo apt install sqlite3`. Alternatively the binary can expose
`kanban backup <path>` using SQLite's `VACUUM INTO`, which avoids the dependency. That's the plan for M5.)

## One-time setup checklist

```bash
# Rust toolchain (user-level, no sudo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Service user + firewall (sudo)
sudo useradd --system --home /var/lib/kanban --shell /usr/sbin/nologin kanban
sudo ufw allow from 192.168.4.0/22 to any port 8080 proto tcp comment 'kanban LAN'
```
