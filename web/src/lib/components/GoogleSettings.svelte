<!-- Settings → Google: connect (paste-back OAuth, docs/google-setup.md), choose calendars,
     sync, push, disconnect. Each of us connects our own account. -->
<script lang="ts">
  import { autofocus } from '../actions';
  import { api, type CalendarOption } from '../api';
  import { google, loadGoogle, push, since, syncNow } from '../google.svelte';
  import { failed, toast } from '../state.svelte';

  /** 'idle' → 'waiting' (Google tab open, waiting for the pasted address) → connected. */
  let step = $state<'idle' | 'waiting'>('idle');
  let authUrl = $state('');
  let pasted = $state('');
  let busy = $state(false);

  let options = $state<CalendarOption[] | null>(null);
  let tasksChoice = $state<string>('');
  let overlayChoice = $state<string[]>([]);
  let saving = $state(false);

  const NEW = 'new';

  $effect(() => {
    loadGoogle().then(() => {
      if (google.status?.connected) loadOptions();
    });
  });

  async function loadOptions() {
    try {
      options = await api.googleCalendars();
      const tasks = options.find((c) => c.role === 'tasks');
      const kanban = options.find((c) => c.writable && c.summary === 'Kanban');
      // Default: the calendar in use, else an existing "Kanban" one, else a new one.
      tasksChoice = tasks?.id ?? kanban?.id ?? NEW;
      overlayChoice = options.filter((c) => c.role === 'overlay').map((c) => c.id);
      if (!google.status?.calendars.length) {
        // First time: suggest the primary calendar as an overlay.
        overlayChoice = options.filter((c) => c.primary).map((c) => c.id);
      }
    } catch (e) {
      failed(e);
    }
  }

  async function connect() {
    // Open the tab synchronously, so the popup blocker allows it, then point it at Google.
    const tab = window.open('about:blank', '_blank');
    busy = true;
    try {
      authUrl = (await api.googleStart()).auth_url;
      if (tab) tab.location.href = authUrl;
      step = 'waiting';
      pasted = '';
    } catch (e) {
      tab?.close();
      failed(e);
    } finally {
      busy = false;
    }
  }

  async function finish(e: SubmitEvent) {
    e.preventDefault();
    if (!pasted.trim()) return;
    busy = true;
    try {
      google.status = await api.googleFinish(pasted.trim());
      step = 'idle';
      toast(`Connected ${google.status.email}`);
      await loadOptions();
    } catch (err) {
      failed(err);
    } finally {
      busy = false;
    }
  }

  async function saveCalendars() {
    saving = true;
    try {
      google.status = await api.googleChooseCalendars({
        tasks: tasksChoice || null,
        overlays: overlayChoice.filter((id) => id !== tasksChoice),
      });
      toast('Calendars saved');
      await loadOptions();
    } catch (e) {
      failed(e);
    } finally {
      saving = false;
    }
  }

  async function disconnect() {
    if (
      !confirm(
        'Disconnect Google? The app stops syncing and forgets your calendar choices. Events already pushed stay in Google Calendar.',
      )
    )
      return;
    try {
      await api.googleDisconnect();
      options = null;
      await loadGoogle();
      toast('Google disconnected');
    } catch (e) {
      failed(e);
    }
  }

  const s = $derived(google.status);
  const tasksCalendar = $derived(s?.calendars.find((c) => c.role === 'tasks'));
  const writable = $derived(options?.filter((c) => c.writable) ?? []);
  const hasKanban = $derived(writable.some((c) => c.summary === 'Kanban'));
  const dirty = $derived.by(() => {
    if (!options || !s) return false;
    const was = s.calendars.filter((c) => c.role === 'overlay').map((c) => c.google_calendar_id).sort().join('\n');
    const now = overlayChoice.filter((id) => id !== tasksChoice).sort().join('\n');
    return (tasksCalendar?.google_calendar_id ?? '') !== (tasksChoice === NEW ? '-' : tasksChoice) || was !== now;
  });
</script>

<section class="panel">
  <h2>Google</h2>

  {#if !s}
    <p class="hint">Loading…</p>
  {:else if !s.configured}
    <p class="hint">
      Google isn't set up on the server yet. Follow <code>docs/google-setup.md</code>, then restart the app
      (<code>sudo systemctl restart kanban</code>).
    </p>
  {:else if !s.connected}
    {#if step === 'idle'}
      <p class="hint">
        Connect your own Google account to see your calendars in the Calendar view and to put your tasks on a
        Google calendar when you press <strong>Push to Google</strong>. Nothing is written to Google until you do.
      </p>
      <button class="btn primary" disabled={busy} onclick={connect}>Connect Google</button>
    {:else}
      <ol class="steps">
        <li>
          In the Google tab, choose your account and allow access. If Google says the app isn't verified, click
          <strong>Advanced → Go to Kanban</strong>. (Tab didn't open?
          <a href={authUrl} target="_blank" rel="noopener noreferrer">Open Google</a>.)
        </li>
        <li>
          Google then sends the tab to <code class="mono">{s.redirect_uri}…</code>, which <strong>won't load</strong>.
          That's expected.
        </li>
        <li>Copy the whole address from that tab's address bar and paste it here:</li>
      </ol>
      <form class="paste" onsubmit={finish}>
        <input
          class="input mono"
          placeholder="http://127.0.0.1:8642/?state=…&code=…"
          bind:value={pasted}
          use:autofocus
          spellcheck="false"
          autocomplete="off"
        />
        <button class="btn primary" disabled={busy || !pasted.trim()}>{busy ? 'Connecting…' : 'Finish'}</button>
        <button type="button" class="btn" onclick={() => (step = 'idle')}>Cancel</button>
      </form>
    {/if}
  {:else}
    <div class="account">
      <span>Connected as <strong>{s.email}</strong></span>
      <button class="btn danger" onclick={disconnect}>Disconnect</button>
    </div>

    {#if s.last_error}
      <p class="error" role="alert">{s.last_error}</p>
      {#if s.last_error.includes('Reconnect')}
        <button class="btn primary" onclick={connect}>Reconnect</button>
      {/if}
    {/if}

    {#if options === null}
      <p class="hint">Loading your calendars…</p>
    {:else}
      <fieldset>
        <legend class="label">Push my tasks to</legend>
        {#if !hasKanban}
          <label>
            <input type="radio" name="tasks-cal" value={NEW} bind:group={tasksChoice} />
            A new calendar called “Kanban”
            <span class="muted">(recommended: easy to show or hide in Google)</span>
          </label>
        {/if}
        {#each writable as c (c.id)}
          <label>
            <input type="radio" name="tasks-cal" value={c.id} bind:group={tasksChoice} />
            <span class="swatch" style:background={c.color ?? 'var(--c-slate)'}></span>
            {c.summary}{c.primary ? ' (primary)' : ''}
          </label>
        {/each}
        <label>
          <input type="radio" name="tasks-cal" value="" bind:group={tasksChoice} />
          Nowhere for now
        </label>
        <p class="hint">
          Tasks assigned to you that have a due date become all-day events there when you press Push to Google. Moving
          or deleting them in Google changes the task's due date here.
        </p>
      </fieldset>

      <fieldset>
        <legend class="label">Show in the Calendar view</legend>
        {#each options.filter((c) => c.id !== tasksChoice) as c (c.id)}
          <label>
            <input type="checkbox" value={c.id} bind:group={overlayChoice} />
            <span class="swatch" style:background={c.color ?? 'var(--c-slate)'}></span>
            {c.summary}{c.primary ? ' (primary)' : ''}
          </label>
        {/each}
        <p class="hint">Read-only, refreshed every 15 minutes. Only you see your calendars.</p>
      </fieldset>

      <button class="btn primary" disabled={saving || !dirty} onclick={saveCalendars}>
        {saving ? 'Saving…' : 'Save calendars'}
      </button>
    {/if}

    <div class="sync">
      <div>
        <span class="label">Pulled from Google</span>
        <span>{since(s.last_sync_at)}</span>
        <button class="btn" disabled={google.syncing} onclick={() => syncNow()}>{google.syncing ? 'Syncing…' : 'Sync now'}</button>
      </div>
      {#if tasksCalendar}
        <div>
          <span class="label">Pushed to “{tasksCalendar.summary}”</span>
          <span>{since(s.last_push_at)}</span>
          <button class="btn primary" disabled={google.pushing || s.pending === 0} onclick={push}>
            {google.pushing ? 'Pushing…' : s.pending ? `Push to Google (${s.pending})` : 'Up to date'}
          </button>
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .panel {
    padding: 18px 20px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1rem;
  }

  .label {
    font-family: var(--font-mono);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .hint {
    margin: 0 0 12px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .steps {
    margin: 0 0 12px;
    padding-left: 20px;
    font-size: 0.9rem;
    display: grid;
    gap: 6px;
  }

  .paste {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .paste .input {
    flex: 1 1 260px;
    font-size: 0.8rem;
  }

  .account {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }

  .error {
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px solid var(--bad);
    border-radius: var(--radius-sm);
    color: var(--bad);
    font-size: 0.9rem;
  }

  fieldset {
    margin: 0 0 14px;
    padding: 0;
    border: 0;
    display: grid;
    gap: 4px;
  }

  legend {
    margin-bottom: 6px;
    padding: 0;
  }

  fieldset label {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 3px 0;
  }

  fieldset .hint {
    margin: 4px 0 0;
  }

  .muted {
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .swatch {
    width: 12px;
    height: 12px;
    border-radius: 3px;
  }

  .sync {
    display: grid;
    gap: 8px;
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .sync div {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .sync .label {
    min-width: 160px;
  }

  .sync .btn {
    margin-left: auto;
  }
</style>
