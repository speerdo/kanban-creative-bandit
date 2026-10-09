<!-- The colored status pill; clicking it opens a menu of the project's statuses. -->
<script lang="ts">
  import type { Status } from '../api';
  import { ink, solid, tint } from '../colors';
  import Popover from './Popover.svelte';

  let {
    status,
    statuses,
    onchange,
  }: { status: Status; statuses: Status[]; onchange: (statusId: number) => void } = $props();

  let open = $state(false);
</script>

<Popover bind:open>
  {#snippet trigger()}
    <button
      class="pill"
      style:background={tint(status.color, 20)}
      style:color={ink(status.color)}
      aria-haspopup="menu"
      aria-expanded={open}
      onclick={() => (open = !open)}
    >
      <span class="dot" style:background={solid(status.color)}></span>{status.name}
    </button>
  {/snippet}
  {#each statuses as s (s.id)}
    <button
      class="item"
      role="menuitemradio"
      aria-checked={s.id === status.id}
      onclick={() => {
        open = false;
        if (s.id !== status.id) onchange(s.id);
      }}
    >
      <span class="dot" style:background={solid(s.color)}></span>{s.name}
      {#if s.id === status.id}<span class="check" aria-hidden="true">✓</span>{/if}
    </button>
  {/each}
</Popover>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 150px;
    padding: 2px 10px 2px 8px;
    border: 0;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .check {
    margin-left: auto;
    color: var(--text-muted);
  }
</style>
