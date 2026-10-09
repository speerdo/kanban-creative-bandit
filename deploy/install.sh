#!/usr/bin/env bash
# Install or upgrade the kanban service. Run with sudo:
#   sudo deploy/install.sh target/release/kanban
# Idempotent: safe to re-run for every upgrade.
set -euo pipefail

BIN_SRC="${1:-target/release/kanban}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LAN_SUBNET="${LAN_SUBNET:-192.168.4.0/22}"
PORT=8080

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo." >&2
  exit 1
fi
if [[ ! -x "$BIN_SRC" ]]; then
  echo "Binary not found at $BIN_SRC. Run 'make release' first." >&2
  exit 1
fi

if ! id kanban &>/dev/null; then
  useradd --system --home-dir /var/lib/kanban --no-create-home --shell /usr/sbin/nologin kanban
  echo "Created system user 'kanban'."
fi

# Back up the live database before replacing the binary (new versions may migrate it).
if [[ -f /var/lib/kanban/kanban.db && -x /usr/local/bin/kanban ]]; then
  install -d -o kanban -g kanban -m 0750 /var/lib/kanban/backups
  sudo -u kanban KANBAN_DB_PATH=/var/lib/kanban/kanban.db \
    /usr/local/bin/kanban backup /var/lib/kanban/backups/pre-upgrade.db >/dev/null
  echo "Saved /var/lib/kanban/backups/pre-upgrade.db"
fi

install -m 0755 "$BIN_SRC" /usr/local/bin/kanban
for unit in kanban.service kanban-backup.service kanban-backup.timer; do
  install -m 0644 "$SCRIPT_DIR/$unit" "/etc/systemd/system/$unit"
done
# Google: hand the token key to the service as a credential, when there is one.
DROPIN=/etc/systemd/system/kanban.service.d/google.conf
if [[ -f /etc/kanban/token.key ]]; then
  if [[ "$(stat -c %s /etc/kanban/token.key)" -ne 32 ]]; then
    echo "Warning: /etc/kanban/token.key must be exactly 32 bytes; Google stays off. See docs/google-setup.md." >&2
  fi
  install -d -m 0755 "$(dirname "$DROPIN")"
  printf '[Service]\nLoadCredential=token-key:/etc/kanban/token.key\n' > "$DROPIN"
else
  rm -f "$DROPIN"
fi

systemctl daemon-reload
systemctl enable kanban.service >/dev/null
systemctl enable --now kanban-backup.timer >/dev/null
systemctl restart kanban.service

if command -v ufw &>/dev/null && ufw status | grep -q "Status: active"; then
  ufw allow from "$LAN_SUBNET" to any port "$PORT" proto tcp comment 'kanban LAN' >/dev/null
  echo "Firewall: allowed $LAN_SUBNET -> :$PORT."
else
  echo "Note: ufw is not active, so :$PORT is reachable by anything that can reach this host."
fi

sleep 1
if curl -fsS "http://127.0.0.1:$PORT/api/health" >/dev/null; then
  echo "kanban is running: http://$(hostname -I | awk '{print $1}'):$PORT"
  echo "Nightly backups: $(systemctl list-timers kanban-backup.timer --no-legend | awk '{print "next run " $1, $2, $3}')"
  if curl -fsS "http://127.0.0.1:$PORT/api/health" | grep -q '"google":"on"'; then
    echo "Google: on (each person connects in Settings → Google)"
  else
    echo "Google: off. $(journalctl -u kanban -n 30 --no-pager -o cat | grep -o 'Google integration off.*' | tail -1)"
  fi
  if ! sudo -u kanban KANBAN_DB_PATH=/var/lib/kanban/kanban.db /usr/local/bin/kanban user list | grep -q .; then
    echo "No accounts yet. Create them with:"
    echo "  sudo -u kanban KANBAN_DB_PATH=/var/lib/kanban/kanban.db /usr/local/bin/kanban user add adam \"Adam\""
  fi
else
  echo "Service did not respond. Check: journalctl -u kanban -n 50" >&2
  exit 1
fi
