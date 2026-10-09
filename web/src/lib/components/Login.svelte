<script lang="ts">
  import { api } from '../api';
  import { session } from '../state.svelte';

  let username = $state('');
  let password = $state('');
  let error = $state('');
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      session.me = await api.login(username, password);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      password = '';
    } finally {
      busy = false;
    }
  }
</script>

<main>
  <form class="card" onsubmit={submit}>
    <img src="/favicon.svg" alt="" width="40" height="40" />
    <h1>Kanban</h1>

    <label>
      <span>Username</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={username} autocomplete="username" autocapitalize="none" required autofocus />
    </label>
    <label>
      <span>Password</span>
      <input class="input" type="password" bind:value={password} autocomplete="current-password" required />
    </label>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <button class="btn primary" type="submit" disabled={busy}>{busy ? 'Signing in…' : 'Sign in'}</button>
  </form>
</main>

<style>
  main {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 16px;
  }

  .card {
    display: grid;
    gap: 14px;
    width: 100%;
    max-width: 340px;
    padding: 32px 28px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  img {
    justify-self: center;
  }

  h1 {
    margin: 0 0 6px;
    text-align: center;
    font-size: 1.5rem;
  }

  label {
    display: grid;
    gap: 4px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .error {
    margin: 0;
    color: var(--bad);
    font-size: 0.875rem;
  }

  button {
    justify-content: center;
    padding: 9px;
  }
</style>
