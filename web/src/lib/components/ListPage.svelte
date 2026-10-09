<!-- One shared list (Groceries, Household…). Built for a phone in one hand: the add box stays
     focused after Enter, a tap checks an item off, checked items sink into their own group, and
     the other phone's changes appear live. -->
<script lang="ts">
  import Sortable from 'sortablejs';
  import { autofocus } from '../actions';
  import { api, type ListItem, type ShoppingList } from '../api';
  import { solid } from '../colors';
  import { between } from '../position';
  import { listen } from '../live.svelte';
  import { failed, go, toast, userById, workspace } from '../state.svelte';
  import Avatar from './Avatar.svelte';
  import ColorSwatches from './ColorSwatches.svelte';
  import Popover from './Popover.svelte';

  let { list }: { list: ShoppingList } = $props();

  let items = $state<ListItem[]>([]);
  let loading = $state(true);
  let draft = $state('');
  let showChecked = $state(false);
  let editing = $state<number | null>(null);
  let editText = $state('');
  let menuOpen = $state(false);
  let renaming = $state(false);
  let name = $state('');

  const byPosition = (a: ListItem, b: ListItem) =>
    a.position < b.position ? -1 : a.position > b.position ? 1 : a.id - b.id;
  const open = $derived(items.filter((i) => !i.checked_at).sort(byPosition));
  const checked = $derived(
    items.filter((i) => i.checked_at).sort((a, b) => (b.checked_at! < a.checked_at! ? -1 : 1)),
  );

  async function load() {
    try {
      items = await api.listItems(list.id);
    } catch (e) {
      failed(e);
    } finally {
      loading = false;
    }
  }

  function upsert(item: ListItem) {
    const known = items.find((i) => i.id === item.id);
    if (known) Object.assign(known, item);
    else items.push(item);
  }

  $effect(() => {
    void list.id;
    loading = true;
    load();
    try {
      localStorage.setItem('lists.last', String(list.id));
    } catch {
      // Not remembered; harmless.
    }
    return listen({
      event(kind, data) {
        if (data.list_id !== list.id && data.id !== list.id) return;
        if (kind === 'list_item.updated' && data.list_id === list.id) upsert(data as unknown as ListItem);
        else if (kind === 'list_item.deleted') items = items.filter((i) => i.id !== data.id);
        else if (kind === 'list.cleared' && data.id === list.id) items = items.filter((i) => !i.checked_at);
      },
      resync: load,
    });
  });

  // ---- add -----------------------------------------------------------------------------------

  let box: HTMLTextAreaElement | undefined = $state();

  async function add() {
    const text = draft.trim();
    if (!text) return;
    draft = '';
    resize();
    try {
      for (const item of await api.addListItems(list.id, text)) upsert(item);
    } catch (e) {
      draft = text;
      failed(e);
    }
    box?.focus();
  }

  function onKey(e: KeyboardEvent) {
    // Enter adds; Shift+Enter starts another line (a pasted list keeps its lines).
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      add();
    }
  }

  function resize() {
    if (!box) return;
    box.style.height = 'auto';
    box.style.height = `${Math.min(box.scrollHeight, 200)}px`;
  }

  // ---- change --------------------------------------------------------------------------------

  async function toggle(item: ListItem) {
    const before = $state.snapshot(item);
    item.checked_at = item.checked_at ? null : new Date().toISOString();
    try {
      upsert(await api.updateListItem(item.id, { checked: !!item.checked_at }));
    } catch (e) {
      Object.assign(item, before);
      failed(e);
    }
  }

  function startEdit(item: ListItem) {
    editing = item.id;
    editText = item.text;
  }

  async function saveEdit(item: ListItem) {
    const text = editText.trim();
    editing = null;
    if (!text || text === item.text) return;
    const before = item.text;
    item.text = text;
    try {
      upsert(await api.updateListItem(item.id, { text }));
    } catch (e) {
      item.text = before;
      failed(e);
    }
  }

  async function remove(item: ListItem) {
    editing = null;
    items = items.filter((i) => i.id !== item.id);
    try {
      await api.deleteListItem(item.id);
    } catch (e) {
      items.push(item);
      failed(e);
    }
  }

  async function clearChecked() {
    const gone = checked.length;
    const before = items;
    items = items.filter((i) => !i.checked_at);
    try {
      await api.clearChecked(list.id);
      toast(`Cleared ${gone} checked item${gone === 1 ? '' : 's'}`);
    } catch (e) {
      items = before;
      failed(e);
    }
  }

  // Drag by the grip to reorder open items. Svelte owns the DOM, so Sortable's move is undone
  // and reported.
  function reorder(node: HTMLElement) {
    let anchor: Node | null = null;
    const instance = Sortable.create(node, {
      handle: '.grip',
      draggable: '[data-item]',
      animation: 150,
      delay: 120,
      delayOnTouchOnly: true,
      ghostClass: 'drag-ghost',
      onStart(evt) {
        anchor = evt.item.nextSibling;
      },
      async onEnd(evt) {
        node.insertBefore(evt.item, anchor);
        anchor = null;
        const index = evt.newDraggableIndex;
        const id = Number(evt.item.dataset.item);
        const item = items.find((i) => i.id === id);
        if (index === undefined || !item || evt.oldDraggableIndex === index) return;
        const others = open.filter((i) => i.id !== id);
        const prev = others[index - 1];
        const next = others[index];
        const before = item.position;
        try {
          item.position = between(prev?.position ?? null, next?.position ?? null);
        } catch {
          // Neighbours collided; the server's answer sorts it out.
        }
        try {
          upsert(await api.updateListItem(id, prev ? { after_id: prev.id } : next ? { before_id: next.id } : {}));
        } catch (e) {
          item.position = before;
          failed(e);
        }
      },
    });
    return { destroy: () => instance.destroy() };
  }

  // ---- the list itself -----------------------------------------------------------------------

  async function rename() {
    renaming = false;
    const n = name.trim();
    if (!n || n === list.name) return;
    try {
      Object.assign(list, await api.updateList(list.id, { name: n }));
    } catch (e) {
      failed(e);
    }
  }

  async function recolor(color: string | null) {
    if (!color) return;
    try {
      Object.assign(list, await api.updateList(list.id, { color }));
    } catch (e) {
      failed(e);
    }
  }

  async function removeList() {
    menuOpen = false;
    if (!confirm(`Delete the list “${list.name}” and its ${items.length} items?`)) return;
    try {
      await api.deleteList(list.id);
      workspace.lists = workspace.lists.filter((l) => l.id !== list.id);
      go({ name: 'list', id: 0 });
    } catch (e) {
      failed(e);
    }
  }
</script>

<header class="head" style:--list={solid(list.color)}>
  <span class="dot" aria-hidden="true"></span>
  {#if renaming}
    <input
      class="input name-input"
      bind:value={name}
      use:autofocus
      maxlength="60"
      onblur={rename}
      onkeydown={(e) => {
        if (e.key === 'Enter') e.currentTarget.blur();
        if (e.key === 'Escape') renaming = false;
      }}
    />
  {:else}
    <h1>
      <button class="name" title="Rename" onclick={() => ((name = list.name), (renaming = true))}>{list.name}</button>
    </h1>
  {/if}
  <span class="count mono">{open.length} to get</span>
  <Popover bind:open={menuOpen} align="right">
    {#snippet trigger()}
      <button class="icon-btn" aria-label="List options" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>⋯</button>
    {/snippet}
    <div class="menu">
      <span class="label">Color</span>
      <ColorSwatches value={list.color} onpick={recolor} />
      <button class="btn danger" onclick={removeList}>Delete list</button>
    </div>
  </Popover>
</header>

<div class="page">
  <form
    class="adder"
    onsubmit={(e) => {
      e.preventDefault();
      add();
    }}
  >
    <textarea
      bind:this={box}
      bind:value={draft}
      rows="1"
      placeholder="Add an item…"
      aria-label="Add an item"
      enterkeyhint="enter"
      autocomplete="off"
      oninput={resize}
      onkeydown={onKey}
    ></textarea>
    <button class="btn primary" disabled={!draft.trim()}>Add</button>
  </form>
  <p class="tip">Paste several lines to add several items. Adding something you've checked off before brings it back.</p>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if items.length === 0}
    <div class="empty">
      <p><strong>Nothing on this list yet.</strong></p>
      <p class="muted">Whatever one of you adds shows up on the other's phone straight away.</p>
    </div>
  {/if}

  <ul class="items" use:reorder>
    {#each open as item (item.id)}
      <li data-item={item.id}>
        {#if editing === item.id}
          <input
            class="input edit"
            bind:value={editText}
            use:autofocus
            maxlength="200"
            onblur={() => saveEdit(item)}
            onkeydown={(e) => {
              if (e.key === 'Enter') e.currentTarget.blur();
              if (e.key === 'Escape') editing = null;
            }}
          />
          <button class="icon-btn" aria-label="Delete {item.text}" onmousedown={(e) => e.preventDefault()} onclick={() => remove(item)}>🗑</button>
        {:else}
          <span class="grip" aria-hidden="true" title="Drag to reorder">⋮⋮</span>
          <button class="item" onclick={() => toggle(item)} aria-pressed="false">
            <span class="box" aria-hidden="true"></span>
            <span class="text">{item.text}</span>
          </button>
          <button class="icon-btn edit-btn" aria-label="Edit {item.text}" title="Edit" onclick={() => startEdit(item)}>✎</button>
        {/if}
      </li>
    {/each}
  </ul>

  {#if checked.length}
    <div class="checked-head">
      <button class="toggle" aria-expanded={showChecked} onclick={() => (showChecked = !showChecked)}>
        <span class="chev" class:open={showChecked}>›</span> Checked ({checked.length})
      </button>
      <button class="btn" onclick={clearChecked}>Clear checked</button>
    </div>
    {#if showChecked}
      <ul class="items done">
        {#each checked as item (item.id)}
          <li>
            <button class="item" onclick={() => toggle(item)} aria-pressed="true" title="Tap to bring it back">
              <span class="box on" aria-hidden="true">✓</span>
              <span class="text">{item.text}</span>
            </button>
            {#if item.checked_by}<Avatar user={userById(item.checked_by)} size={20} />{/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .head {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 24px;
    background: color-mix(in oklch, var(--surface) 88%, transparent);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 3px 0 var(--list);
  }

  .dot {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 4px;
    background: var(--list);
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
    min-width: 0;
  }

  .name {
    border: 0;
    background: transparent;
    padding: 0;
    font: inherit;
    color: inherit;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }

  .name-input {
    font-size: 1.1rem;
    max-width: 320px;
  }

  .count {
    margin-left: auto;
    font-size: 0.75rem;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .menu {
    display: grid;
    gap: 10px;
    padding: 12px;
    width: 250px;
  }

  .label {
    font-family: var(--font-mono);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .page {
    max-width: 640px;
    margin: 16px 24px 96px;
    padding: 14px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .adder {
    display: flex;
    gap: 8px;
    align-items: flex-end;
  }

  textarea {
    flex: 1;
    resize: none;
    min-height: 46px;
    padding: 11px 12px;
    font: inherit;
    font-size: 1.05rem;
    line-height: 1.4;
    color: var(--text);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }

  textarea:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }

  .adder .btn {
    min-height: 46px;
    padding-inline: 16px;
  }

  .tip {
    margin: 6px 2px 12px;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .items {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .items li {
    display: flex;
    align-items: center;
    gap: 4px;
    border-bottom: 1px solid var(--border);
    min-height: 50px;
  }

  .grip {
    flex: none;
    width: 18px;
    color: var(--text-muted);
    cursor: grab;
    font-size: 0.8rem;
    letter-spacing: -3px;
    opacity: 0.5;
    touch-action: none;
    user-select: none;
  }

  .item {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    min-height: 50px;
    padding: 6px 4px;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 1.05rem;
    text-align: left;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .text {
    overflow-wrap: anywhere;
  }

  .box {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 2px solid var(--text-muted);
    border-radius: 6px;
    font-size: 0.85rem;
    font-weight: 700;
  }

  .box.on {
    background: var(--ok);
    border-color: var(--ok);
    color: var(--surface);
  }

  .item:hover .box:not(.on) {
    border-color: var(--accent);
  }

  .edit-btn {
    opacity: 0;
  }

  li:hover .edit-btn,
  .edit-btn:focus-visible {
    opacity: 1;
  }

  .edit {
    flex: 1;
    font-size: 1rem;
  }

  .checked-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 14px;
  }

  .toggle {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.9rem;
    padding: 6px 0;
  }

  .chev {
    display: inline-block;
    transition: transform 0.15s;
  }

  .chev.open {
    transform: rotate(90deg);
  }

  .done .text {
    color: var(--text-muted);
    text-decoration: line-through;
  }

  .empty {
    padding: 24px 4px 8px;
    text-align: center;
  }

  .muted {
    color: var(--text-muted);
  }

  @media (max-width: 760px) {
    .head {
      padding-left: 52px;
    }

    .page {
      margin: 8px 6px 96px;
      padding: 10px 10px;
    }

    /* No hover on phones: keep the edit button visible, but quiet. */
    .edit-btn {
      opacity: 0.45;
    }

    .grip {
      opacity: 0.35;
    }
  }
</style>
