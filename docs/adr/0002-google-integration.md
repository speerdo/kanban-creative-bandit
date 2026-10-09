# ADR 0002: Google integration

- **Status:** Accepted. Calendar was built in M6; Gmail, Drive, Lists and the Keep import in M7.
- **Date:** 2026-10-09
- **Deciders:** Adam, Kat

## Context

We both live in Google: Calendar, Gmail, Keep and Drive. The kanban app should meet us there:

- Changes in our **Google calendars** should show up in the app automatically (at least daily).
- Tasks with due dates should be **pushable to Google Calendar on demand**: no automatic writes into
  Google, just a button.
- The app should have its own **calendar view** that also shows our Google events, so we can plan around them.
- An email that needs action should become a task without copy and paste (**Gmail**).
- Tasks often point at Docs, Sheets and Drive folders (**Drive**).
- The shared grocery and household lists live in **Keep**. Both of us want the same list on
  our phones. Keep itself is a nice-to-have. A shared list that only lives in the app is fine.

Constraints that shape the design (checked against Google's docs on 2026-10-09):

1. **The app is LAN-only on plain HTTP** (`http://192.168.4.32:8080`). Google's web-server OAuth
   flow rejects redirect URIs that are raw IP addresses or non-HTTPS, apart from `localhost`.
2. The **device flow** ("TVs and limited-input devices") only allows sign-in, `drive.file` and
   YouTube scopes. **Calendar is not allowed**, so it can't be used.
3. Calendar **push notifications** (webhooks) need a public HTTPS endpoint with a trusted
   certificate. We don't have one.
4. If the OAuth consent screen stays in **Testing** mode, refresh tokens **expire after 7 days**.
5. The **Keep API is Workspace-only.** It requires domain-wide delegation by a Workspace admin and is
   not available for personal `@gmail.com` accounts.
6. The server shares a low-power box with the trading bot, so builds and dependencies stay small
   (see [hosting.md](../hosting.md)).

## Decision

### Account connection: "Desktop app" OAuth client, loopback redirect, paste-back

- Create **one Google Cloud project** ("Kanban") with an OAuth client of type **Desktop app**.
  Google allows loopback redirects (`http://127.0.0.1:<port>`) for this client type, and they
  have no HTTPS requirement.
- In Settings → Google, each of us clicks **Connect Google**. The app opens Google's consent
  page (with PKCE and a `state` value). After you approve, Google redirects your browser to
  `http://127.0.0.1:8642/?code=…`. Nothing listens there, so the page fails to load, but the
  address bar holds the code. **Paste that URL back into the app**, and the server exchanges it
  for a refresh token.
- This happens **once per person**. After that, the server refreshes access tokens by itself.
- Publish the consent screen to **In production**, without verification, so refresh tokens
  don't expire after 7 days. Each of us clicks through one "Google hasn't verified this app"
  warning. Unverified apps are capped at 100 users, which is far more than we need.
- **Upgrade path:** if we ever add Tailscale with HTTPS (`https://<host>.<tailnet>.ts.net`),
  switch to a *Web application* client with a normal redirect. That also makes push
  notifications possible. Nothing else changes.

### Each person connects their own Google account

The connection is stored per user. Adam's tasks sync to Adam's calendar, and Kat's tasks sync to
Kat's. Neither of us needs access to the other's Google data.

### Calendar: Google flows in automatically, the app pushes to Google only on request

Decided with Adam on 2026-10-09: pull from Google automatically, and push to Google only when we
press a button.

- **Tasks → Google, on demand.** Nothing is written to Google until you press **Push to Google**
  (in the Calendar view and in Settings). The button shows how many changes are waiting ("Push 3
  changes"). A push sends every task whose due date, title or completion changed since your last
  push. A task that has a due date and is **assigned to you** becomes an all-day event
  on a calendar you choose. The default is a new secondary calendar named **"Kanban"**, so it's
  easy to toggle in Google Calendar and keeps your primary calendar clean. The event title
  is the task title (prefixed with ✓ when the task is done). The description holds a link back to
  the task. The task id is stored in the event's `extendedProperties.private`.
- **Google → tasks, automatic.** If you drag one of those events to a different day in Google Calendar, the
  task's due date follows. If you delete the event, the task loses its due date (the task itself
  is never deleted from Google). The app stays the source of truth for title, description and
  everything else.
- **Overlay.** Calendars you pick (e.g. your primary calendar and a shared family calendar) are
  read into a local cache and drawn, read-only, in the app's Calendar view next to the tasks.
- **Polling, not webhooks.** A background job pulls every **15 minutes**. That's well within the
  "at least daily" requirement. It also runs on demand from "Sync now", when the Calendar view opens
  (if the last pull is more than 5 minutes old), and before every push.
  - The **tasks calendar** is read incrementally with `events.list` and the stored `syncToken`. An
    unchanged calendar costs one tiny request. A `410 Gone` response triggers a full resync.
  - **Overlay calendars** are re-read for a window, from two months back to about a year ahead,
    with recurring events expanded (`singleEvents`), and the cache is replaced each time.
    *Changed during M6:* a `syncToken` can't be combined with a time window, and without a window
    a calendar's full history and endlessly repeating events would all land in the cache. A
    window costs one request per calendar per pull, which is nothing at our scale.
- **Conflicts:** if a task changed in the app *and* its event changed in Google since the last push,
  the pull applies Google's date and the task shows "changed in Google". The next push then
  sends the app's title and completion. Changes we push are recognised by etag and ignored when they
  come back on the next pull.

### Gmail: label a thread to turn it into a task

- Apply the Gmail label **`Kanban`** to a thread, in any Gmail client including the phone app.
  On the next poll it becomes a task in your chosen inbox project. The subject becomes the title.
  The sender, the snippet and a link that opens the thread in Gmail go into the description.
- Imported thread ids are recorded, so a thread is never imported twice. After import the
  label is swapped for **`Kanban/Imported`**, so Gmail shows what's done.
- Scope: `gmail.modify`. It's a *restricted* scope, which is fine for an unverified personal app.
  It's also the minimum scope that can read a thread and change its labels. This is an
  **optional** part of the connection, requested only when you turn the Gmail feature on.

### Drive: typed links on tasks

- Paste any Google Docs, Sheets, Slides, Forms or Drive URL into a task, and it's stored as a
  **task link** and shown as a chip with the right icon. Parsing the URL needs no API access.
- If you also grant `drive.metadata.readonly` (optional), the app fetches the file's real title
  and type, and refreshes them when a task opens.
- The Google Picker is out for now. It needs an HTTPS JavaScript origin, the same blocker as OAuth.

### Keep: replaced by in-app shared Lists, with an optional Takeout import

- There's **no official Keep API for personal accounts**, so there's no live sync.
- What we actually use Keep for is a **shared grocery and household checklist on both phones**.
  The app provides that natively as **Lists** (see the blueprint). They're quick to add to, there's
  one tap to check an item off, checked items can be cleared, and changes show up live on the other
  phone over SSE. The web app can be installed to the phone's home screen.
- **Prerequisite:** a grocery list is used *at the store*, away from home Wi-Fi. That requires
  remote access, i.e. **Tailscale** on the server and both phones (see [hosting.md](../hosting.md)).
  Tailscale also brings HTTPS, which the home-screen install needs.
- **Optional:** Settings → Import can still take a Takeout export, to seed Lists and tasks from
  existing Keep notes.
- The import accepts a **Google Takeout** Keep export (`.zip` or the folder of `.json` files).
  A checklist note becomes a List, and its items become list items. Any other note becomes a task.
  Archived and trashed notes are skipped by default.
- Rejected: the unofficial `gkeepapi`. It's reverse-engineered, needs a "master token" for your
  whole Google account, breaks without warning, and goes against Google's terms of service.

### Implementation: a plain REST client, not generated SDKs

- Use `reqwest` (rustls) with hand-written `serde` structs for the dozen endpoints we call.
  The generated `google-*3` crates pull in large dependency trees, and that would cost real
  build time on the Celeron.
- Tokens are refreshed lazily (when an access token is within 60 seconds of expiry) and cached in memory.
- Refresh tokens are **encrypted at rest** (XChaCha20-Poly1305). The key lives in a file
  passed in by systemd `LoadCredential=`, so a copied database file or backup can't be used
  to reach our Google accounts.
- **Disconnect** revokes the token at Google and deletes everything cached for that user.

### Scopes

| Feature | Scope | Google class | When requested |
|---|---|---|---|
| Identify the account | `openid email` | basic | Connect |
| Calendar sync + overlay | `calendar` | sensitive | Connect |
| Gmail → task | `gmail.modify` | restricted | Turning on the Gmail feature |
| Drive titles | `drive.metadata.readonly` | restricted | Turning on Drive titles |

`calendar` (rather than `calendar.events` + `calendar.readonly`) is needed to **create** the
"Kanban" secondary calendar. Additional scopes use incremental authorization
(`include_granted_scopes=true`), so turning a feature on repeats the paste-back once.

## Alternatives considered

- **Sign in with Google instead of passwords.** It hits the same redirect problem, and the password login
  already works. We can revisit it with the HTTPS upgrade.
- **iCal feed (read-only `.ics` URL) instead of the API.** It works without OAuth, but Google
  fetches subscribed feeds only every 8 to 24 hours and has to reach the server from the internet.
  It's also one-way only.
- **One shared Google account or calendar for both of us.** It's simpler to build, but it means
  sharing credentials or having one person's account own the other's data. Per-user connections
  avoid both. A shared "Kanban" calendar can still be created by one of us and shared in Google.
- **Google Tasks API as a bridge for Keep.** It's possible later if we start using Google Tasks.
  It's not worth building on speculation.

## Consequences

- The server makes **outbound HTTPS** calls to `*.googleapis.com` and `oauth2.googleapis.com`.
  Inbound exposure doesn't change: it's still LAN-only.
- New one-time setup: a Google Cloud project, an OAuth client and a key file. See
  [google-setup.md](../google-setup.md).
- Calendar changes reach the app within about 15 minutes, not instantly. Changes made in the app reach Google
  only when someone presses Push to Google.
- Gmail and Drive scopes are optional. Calendar works without them.
- Adds `reqwest` (rustls) and `chacha20poly1305` to the build.
