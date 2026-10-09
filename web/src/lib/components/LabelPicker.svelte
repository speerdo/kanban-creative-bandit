<!-- Toggle labels on a task; typing a new name and pressing Enter creates it. -->
<script lang="ts">
  import { autofocus } from '../actions';
  import { api } from '../api';
  import { solid } from '../colors';
  import { failed, workspace } from '../state.svelte';

  let { selected, onchange }: { selected: number[]; onchange: (ids: number[]) => void } = $props();

  let query = $state('');
  const matches = $derived(workspace.labels.filter((l) => l.name.toLowerCase().includes(query.trim().toLowerCase())));
  const exact = $derived(workspace.labels.some((l) => l.name.toLowerCase() === query.trim().toLowerCase()));

  function toggle(id: number) {
    onchange(selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id]);
  }

  const COLORS = ['blue', 'green', 'violet', 'orange', 'teal', 'pink', 'amber', 'cyan', 'red', 'lime'];

  async function create() {
    const name = query.trim();
    if (!name || exact) return;
    try {
      const label = await api.createLabel({ name, color: COLORS[workspace.labels.length % COLORS.length] });
      if (!workspace.labels.some((l) => l.id === label.id)) workspace.labels.push(label);
      onchange([...selected, label.id]);
      query = '';
    } catch (e) {
      failed(e);
    }
  }
</script>

<div class="picker">
  <input
    class="input"
    placeholder="Find or create a label"
    bind:value={query}
    use:autofocus
    onkeydown={(e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        if (matches.length === 1 && exact) toggle(matches[0].id);
        else create();
      }
    }}
  />
  <div class="options" role="listbox" aria-multiselectable="true">
    {#each matches as l (l.id)}
      <button class="item" role="option" aria-selected={selected.includes(l.id)} onclick={() => toggle(l.id)}>
        <span class="box">{selected.includes(l.id) ? '✓' : ''}</span>
        <span class="dot" style:background={solid(l.color)}></span>
        {l.name}
      </button>
    {/each}
    {#if query.trim() && !exact}
      <button class="item create" onclick={create}>＋ Create “{query.trim()}”</button>
    {/if}
    {#if workspace.labels.length === 0 && !query.trim()}
      <p class="muted empty">No labels yet. Type a name to create one.</p>
    {/if}
  </div>
</div>

<style>
  .picker {
    width: 230px;
    display: grid;
    gap: 6px;
  }

  .options {
    max-height: 240px;
    overflow-y: auto;
  }

  .box {
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    border: 1.5px solid var(--border);
    border-radius: 4px;
    font-size: 11px;
  }

  .dot {
    width: 9px;
    height: 9px;
    border-radius: 3px;
  }

  .create {
    color: var(--accent);
  }

  .empty {
    margin: 4px 8px;
    font-size: 0.8rem;
  }
</style>
