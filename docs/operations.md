# Operations runbook

Day-to-day running of the kanban app on the home server. See [hosting.md](hosting.md) for why it is
set up this way (the trading bot always wins).

All commands run on the server. `kanban` subcommands that touch the live database run as the
`kanban` user, so set this once per shell:

```bash
alias kb='sudo -u kanban KANBAN_DB_PATH=/var/lib/kanban/kanban.db /usr/local/bin/kanban'
```

## Install and upgrade

```bash
cd ~/Projects/kanban-creative-bandit
git pull
make install        # builds at low priority, backs up the DB, installs, restarts, health-checks
```

`make install` is safe to repeat. Every run:

1. Builds the frontend and the release binary with `nice`/`ionice` on 2 cores.
2. Saves `/var/lib/kanban/backups/pre-upgrade.db` with the *old* binary. Migrations run on start, so
   this is the copy to go back to if an upgrade misbehaves.
3. Installs the binary and the systemd units, enables the nightly backup timer, restarts the service,
   and checks `/api/health`.

First install only: create the accounts.

```bash
kb user add adam "Adam"
kb user add kat "Kat"
```

### Rolling back an upgrade

```bash
git checkout <previous commit> && make install    # old binary
sudo systemctl stop kanban
kb restore /var/lib/kanban/backups/pre-upgrade.db # only if the new version migrated the DB
sudo systemctl start kanban
```

## Health

```bash
curl -s http://127.0.0.1:8080/api/health
# {"status":"ok","db":"ok","db_bytes":122880,"uptime_seconds":5321,"version":"0.1.0","google":"on"}
systemctl status kanban
kb check                                          # SQLite integrity check
```

## Logs

The app logs to the systemd journal. journald rotates and caps it system-wide, so there's no separate
log rotation to manage.

```bash
journalctl -u kanban -f                 # follow
journalctl -u kanban --since today
journalctl -u kanban-backup             # backup runs
```

For more detail, add `Environment=RUST_LOG=debug` with `sudo systemctl edit kanban`, then restart.

## Google Calendar

Each of us connects our own Google account in **Settings → Google** (setup:
[google-setup.md](google-setup.md)). After that:

- Every 15 minutes, the server pulls our chosen Google calendars into the Calendar view. It also pulls
  changes to the events we pushed: moving one in Google moves the task's due date, and deleting one
  clears it.
- Nothing goes to Google until someone presses **Push to Google**. A push sends our own dated tasks
  to our "Kanban" calendar as all-day events.
- Outbound only: the server calls `oauth2.googleapis.com` and `www.googleapis.com` over HTTPS.
  Nothing new is exposed.

## Backups

- **Nightly:** the `kanban-backup.timer` timer runs at about 03:30 (with a random delay of up to 15 minutes, and a catch-up after a power cut)
  and keeps the newest **14** in `/var/lib/kanban/backups/`. Each backup is a consistent copy made while the app
  keeps running, at idle CPU/IO priority.
- **On demand:** `sudo systemctl start kanban-backup`, or `kb backup /var/lib/kanban/backups/before-big-change.db`.
- **When:** `systemctl list-timers kanban-backup.timer`.

A copy that never leaves the machine doesn't protect against a dead disk. To keep one elsewhere, add
a root cron job (or a timer) that copies the newest backup off the box, for example to a NAS or a laptop:

```bash
rsync -a /var/lib/kanban/backups/ nas:/backups/kanban/
```

The Google integration (M6) stores tokens encrypted with `/etc/kanban/token.key`. That key is
deliberately **not** part of these backups. Keep a copy of it somewhere safe. Without it, a restored
database still works, but each of us has to reconnect Google.

## Restore

```bash
ls -lt /var/lib/kanban/backups/
sudo systemctl stop kanban
kb restore /var/lib/kanban/backups/kanban-2026-10-09T033512Z.db
sudo systemctl start kanban
```

`kb restore` checks that the file is an intact kanban database before it touches anything. It refuses
to run while the app is listening. It keeps the database it replaces as `kanban.db.before-restore`, so
a restore can itself be undone.

## Troubleshooting

| Symptom | Check |
|---|---|
| Page won't load from another device | `systemctl status kanban`; `sudo ufw status` (port 8080 allowed from 192.168.4.0/22?); is the server's IP still 192.168.4.32? |
| "Frontend not built" | The binary was built without `web/dist`. Run `make install`, which builds the frontend first. |
| Changes don't appear in the other browser live | The dot next to List/Board turns grey while reconnecting. Reloading the page refetches everything. |
| Locked out | `kb user passwd adam` (this also signs out every session) |
| Service won't start after an upgrade | `journalctl -u kanban -n 50`, then roll back (above) |
| Settings says Google isn't set up | `journalctl -u kanban -o cat \| grep -i google` shows why. Usually `/etc/kanban/google.env` or `token.key` is missing, or the key isn't 32 bytes. Fix it, then `make install` ([google-setup.md](google-setup.md)) |
| "Google access was revoked. Reconnect." | Access was removed at myaccount.google.com, or the token key changed. Settings → Google → Reconnect |
| Google calendar changes don't show up | Calendar view → ↻ (Sync now). Any error appears in red there and in Settings → Google |

## Remote access (needed before shared Lists, M7)

A grocery list has to work at the store, so the phones need to reach the app away from home.
The plan is [Tailscale](https://tailscale.com) rather than opening a port on the router:

1. Install Tailscale on the server and on both phones, and sign in to the same tailnet.
2. In the Tailscale admin console, turn on MagicDNS and HTTPS certificates.
3. On the server, run `sudo tailscale serve --bg 8080`. The app is then available at
   `https://<server-name>.<tailnet>.ts.net` from anywhere, over HTTPS, and only to our devices.
4. HTTPS also unlocks "Add to Home Screen" as an app, and the Google Calendar push and Drive Picker
   upgrades noted in [ADR 0002](adr/0002-google-integration.md).

The LAN address keeps working at home, with no change for the laptops.
