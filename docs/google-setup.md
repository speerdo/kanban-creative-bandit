# Google setup (one time)

These steps get the server ready for the Google integration ([ADR 0002](adr/0002-google-integration.md)).
They're needed from milestone M6 onward. Allow about 15 minutes. Do them once, with whichever Google account
should own the Cloud project. Either of us is fine: the project only holds the app's identity,
not anyone's data.

## 1. Create the Cloud project and enable APIs

1. Go to <https://console.cloud.google.com/> → project picker → **New project** → name it `Kanban`.
2. **APIs & Services → Library**, and enable:
   - Google Calendar API
   - Gmail API (only needed for the Gmail feature)
   - Google Drive API (only needed for Drive titles)

## 2. Consent screen

**APIs & Services → OAuth consent screen** (shown as "Google Auth Platform" in newer consoles):

1. User type: **External**.
2. App name `Kanban`, plus a support email and a developer contact email (your own address is fine).
3. Scopes: you can skip this step. The app asks for scopes at connect time.
4. **Audience → Publish app → In production.** Don't leave it in *Testing*: in Testing mode Google
   expires refresh tokens after 7 days, and the sync would stop every week.
5. Skip verification. While connecting, each of us will see **"Google hasn't verified this app"**.
   Click **Advanced → Go to Kanban (unsafe)**. That's expected for a private app.

## 3. OAuth client

**APIs & Services → Credentials → Create credentials → OAuth client ID**:

- Application type: **Desktop app**. This matters: only Desktop clients may redirect to
  `127.0.0.1`, which is what lets a LAN-only app connect without HTTPS.
- Name: `Kanban server`.
- Copy the **client ID** and **client secret**.

## 4. Give them to the server

```bash
sudo install -d -m 0750 /etc/kanban

# Client id/secret (for a Desktop client, Google treats the secret as non-confidential, but keep it private anyway)
sudo tee /etc/kanban/google.env >/dev/null <<'EOF'
KANBAN_GOOGLE_CLIENT_ID=1234567890-xxxx.apps.googleusercontent.com
KANBAN_GOOGLE_CLIENT_SECRET=GOCSPX-xxxxxxxx
EOF
sudo chmod 0640 /etc/kanban/google.env

# Key that encrypts stored Google refresh tokens (32 random bytes). Back this up somewhere
# safe, separately from the database: without it, both of us have to reconnect.
sudo sh -c 'head -c 32 /dev/urandom > /etc/kanban/token.key'
sudo chmod 0600 /etc/kanban/token.key
```

The systemd unit loads these with `EnvironmentFile=-/etc/kanban/google.env` and
`LoadCredential=token-key:/etc/kanban/token.key`. Both are optional: without them, the app runs
with the Google features turned off.

For local development, put the same two variables in `.env`, and set
`KANBAN_TOKEN_KEY_FILE=data/token.key`.

## 5. Connect (each person)

In the app: **Settings → Integrations → Connect Google**.

1. Approve the requested access on Google's page.
2. Your browser ends up on a page that **fails to load** at `http://127.0.0.1:…/?code=…`. That's expected.
3. Copy the whole URL from the address bar, paste it into the box in Settings, and click **Finish**.
4. Choose the calendar for your tasks (the default creates a new **Kanban** calendar) and which of your
   calendars to show in the app's Calendar view. They're pulled every 15 minutes. Nothing is written to
   Google until you press **Push to Google**.

To disconnect, use **Settings → Integrations → Disconnect**. It revokes access at Google too. You can
also remove access at any time from <https://myaccount.google.com/permissions>.

## Keep import (optional)

Shared grocery and household lists live in the app's **Lists**. To seed them from Keep (there's no
API for personal accounts, so this uses a file):

1. Go to <https://takeout.google.com/>, click **Deselect all**, check **Keep**, and export.
2. In the app, open **Settings → Import → Google Keep** and upload the `.zip`.
