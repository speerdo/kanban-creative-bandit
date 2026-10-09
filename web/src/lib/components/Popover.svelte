<!-- An anchored dropdown. The trigger is rendered by the parent inside `trigger`; the panel
     closes on outside click and on Escape (returning focus to the trigger). -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    open = $bindable(false),
    align = 'left',
    trigger,
    children,
  }: { open?: boolean; align?: 'left' | 'right'; trigger: Snippet; children: Snippet } = $props();

  let root: HTMLElement;

  function onWindowPointer(e: PointerEvent) {
    if (open && !root.contains(e.target as Node)) open = false;
  }

  function onKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      open = false;
      (root.querySelector('button') as HTMLElement | null)?.focus();
    }
  }
</script>

<svelte:window onpointerdown={onWindowPointer} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="wrap" bind:this={root} onkeydown={onKey}>
  {@render trigger()}
  {#if open}
    <div class="panel" class:right={align === 'right'}>
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
  }

  .panel {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 180px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
  }

  .panel.right {
    left: auto;
    right: 0;
  }

  .panel :global(.item) {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    text-align: left;
    white-space: nowrap;
  }

  .panel :global(.item:hover),
  .panel :global(.item:focus-visible) {
    background: var(--hover);
    outline: none;
  }

  .panel :global(.item.danger) {
    color: var(--bad);
  }

  .panel :global(hr) {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
</style>
