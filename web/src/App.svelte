<script lang="ts">
  import { api, type Health } from './lib/api';

  let health = $state<Health | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    api
      .health()
      .then((h) => (health = h))
      .catch((e: unknown) => (error = e instanceof Error ? e.message : String(e)));
  });
</script>

<main>
  <div class="card">
    <img src="/favicon.svg" alt="" width="48" height="48" />
    <h1>Kanban</h1>
    <p class="muted">Hello, Adam &amp; Kat 👋 — the board is coming in M1.</p>

    <div class="status">
      {#if error}
        <span class="dot bad"></span> Server unreachable: {error}
      {:else if !health}
        <span class="dot"></span> Checking server…
      {:else}
        <span class="dot" class:ok={health.status === 'ok'} class:bad={health.status !== 'ok'}></span>
        Server {health.status} · database {health.db} · v{health.version}
      {/if}
    </div>
  </div>
</main>

<style>
  main {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 16px;
  }

  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 40px 32px;
    max-width: 420px;
    width: 100%;
    text-align: center;
  }

  h1 {
    margin: 12px 0 4px;
    font-size: 1.75rem;
  }

  .muted {
    color: var(--text-muted);
    margin: 0 0 24px;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 0.875rem;
    color: var(--text-muted);
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--border);
  }

  .dot.ok {
    background: var(--ok);
  }

  .dot.bad {
    background: var(--bad);
  }
</style>
