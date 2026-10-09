<script lang="ts">
  import { SHORTCUTS } from '../shortcuts';

  let { onclose }: { onclose: () => void } = $props();
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });
</script>

<dialog bind:this={dialog} {onclose} onclick={(e) => e.target === dialog && dialog.close()} aria-label="Keyboard shortcuts">
  <h2>Keyboard shortcuts</h2>
  <dl>
    {#each SHORTCUTS as [keys, what] (keys)}
      <dt>{#each keys.split(' / ') as k, i (k)}{#if i > 0}<span class="or">/</span>{/if}<kbd>{k}</kbd>{/each}</dt>
      <dd>{what}</dd>
    {/each}
  </dl>
  <button class="btn" onclick={() => dialog.close()}>Close</button>
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 32px));
    padding: 20px 22px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 0.3);
  }

  h2 {
    margin: 0 0 14px;
    font-size: 1.05rem;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 16px;
    margin: 0 0 16px;
    align-items: center;
  }

  dt {
    white-space: nowrap;
  }

  dd {
    margin: 0;
    color: var(--text-muted);
  }

  kbd {
    font-family: var(--font-mono);
    font-size: 0.8rem;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 4px;
    background: var(--surface-2);
  }

  .or {
    margin: 0 4px;
    color: var(--text-muted);
  }
</style>
