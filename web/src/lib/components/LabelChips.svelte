<script lang="ts">
  import { ink, tint } from '../colors';
  import { labelById } from '../state.svelte';

  let { ids, max = 3 }: { ids: number[]; max?: number } = $props();

  const labels = $derived(ids.map(labelById).filter((l) => l !== undefined));
</script>

{#if labels.length > 0}
  <span class="chips">
    {#each labels.slice(0, max) as l (l.id)}
      <span class="chip" style:background={tint(l.color, 20)} style:color={ink(l.color)}>{l.name}</span>
    {/each}
    {#if labels.length > max}<span class="more mono">+{labels.length - max}</span>{/if}
  </span>
{/if}

<style>
  .chips {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    min-width: 0;
  }

  .chip {
    padding: 1px 7px;
    border-radius: 4px;
    font-size: 0.72rem;
    font-weight: 600;
    white-space: nowrap;
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .more {
    font-size: 0.7rem;
    color: var(--text-muted);
  }
</style>
