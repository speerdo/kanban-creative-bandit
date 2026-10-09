<script lang="ts">
  import { dismiss, toasts } from '../state.svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}
    <button class="toast" class:error={t.kind === 'error'} onclick={() => dismiss(t.id)}>{t.text}</button>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: grid;
    gap: 8px;
    z-index: 100;
    width: min(420px, calc(100vw - 32px));
  }

  .toast {
    text-align: left;
    padding: 10px 14px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--text);
    color: var(--bg);
    box-shadow: var(--shadow);
  }

  .toast.error {
    background: var(--bad);
    border-color: var(--bad);
    color: #fff;
  }
</style>
