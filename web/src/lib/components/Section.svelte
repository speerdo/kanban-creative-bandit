<!-- One status as a collapsible list section: colored header, its tasks, and an inline add row. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import type { Category, NewTask, Status, Task, TaskPatch } from '../api';
  import { ink, solid } from '../colors';
  import { parseQuickAdd } from '../quickadd';
  import { people } from '../state.svelte';
  import ColorSwatches from './ColorSwatches.svelte';
  import Popover from './Popover.svelte';
  import TaskRow from './TaskRow.svelte';

  let {
    status,
    statuses,
    tasks,
    onupdatetask,
    oncreatetask,
    ondeletetask,
    onupdatestatus,
    onaddstatus,
    ondeletestatus,
  }: {
    status: Status;
    statuses: Status[];
    tasks: Task[];
    onupdatetask: (task: Task, patch: TaskPatch) => void;
    oncreatetask: (t: NewTask) => Promise<boolean>;
    ondeletetask: (task: Task) => void;
    onupdatestatus: (patch: Partial<Pick<Status, 'name' | 'color' | 'category'>>) => void;
    onaddstatus: () => void;
    ondeletestatus: (moveTo?: number) => void;
  } = $props();

  // A status id never changes for a mounted section.
  const KEY = `kanban.collapsed.${untrack(() => status.id)}`;
  let collapsed = $state(read(KEY) === '1');
  let menuOpen = $state(false);
  let menuPage = $state<'main' | 'delete'>('main');
  let renaming = $state(false);
  let draft = $state('');

  function read(k: string) {
    try {
      return localStorage.getItem(k);
    } catch {
      return null;
    }
  }

  function toggle() {
    collapsed = !collapsed;
    try {
      localStorage.setItem(KEY, collapsed ? '1' : '0');
    } catch {
      /* private mode: just don't remember */
    }
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const parsed = parseQuickAdd(draft, people.users);
    if (!parsed.title) return;
    const ok = await oncreatetask({
      status_id: status.id,
      title: parsed.title,
      priority: parsed.priority,
      assignee_id: parsed.assignee?.id,
      due_date: parsed.due_date,
    });
    if (ok) draft = ''; // keep focus for the next one
  }

  function rename(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value.trim();
    renaming = false;
    if (value && value !== status.name) onupdatestatus({ name: value });
  }

  function openMenu() {
    menuPage = 'main';
    menuOpen = !menuOpen;
  }

  function remove(moveTo?: number) {
    menuOpen = false;
    ondeletestatus(moveTo);
  }

  const CATEGORIES: [Category, string][] = [
    ['todo', 'To do'],
    ['in_progress', 'In progress'],
    ['done', 'Done (completes tasks)'],
  ];
  const others = $derived(statuses.filter((s) => s.id !== status.id));
</script>

<section style:--hue={solid(status.color)}>
  <div class="head">
    <button class="icon-btn caret" class:collapsed aria-expanded={!collapsed} aria-label="Collapse {status.name}" onclick={toggle}
      >▾</button
    >
    {#if renaming}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input rename"
        value={status.name}
        autofocus
        onblur={rename}
        onkeydown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
          if (e.key === 'Escape') renaming = false;
        }}
      />
    {:else}
      <h2 style:color={ink(status.color)} ondblclick={() => (renaming = true)}>{status.name}</h2>
    {/if}
    <span class="count">{tasks.length}</span>

    <Popover bind:open={menuOpen}>
      {#snippet trigger()}
        <button class="icon-btn" aria-label="Section menu for {status.name}" onclick={openMenu}>⋯</button>
      {/snippet}
      {#if menuPage === 'main'}
        <button class="item" onclick={() => ((menuOpen = false), (renaming = true))}>Rename</button>
        <ColorSwatches value={status.color} onpick={(c) => onupdatestatus({ color: c })} />
        <hr />
        {#each CATEGORIES as [value, label] (value)}
          <button class="item" role="menuitemradio" aria-checked={status.category === value} onclick={() => onupdatestatus({ category: value })}>
            <span class="radio">{status.category === value ? '●' : '○'}</span>{label}
          </button>
        {/each}
        <hr />
        <button class="item" onclick={() => ((menuOpen = false), onaddstatus())}>Add section below</button>
        <button
          class="item danger"
          disabled={others.length === 0}
          onclick={() => (tasks.length > 0 ? (menuPage = 'delete') : remove())}>Delete section…</button
        >
      {:else}
        <p class="note">Move {tasks.length} task{tasks.length === 1 ? '' : 's'} to:</p>
        {#each others as s (s.id)}
          <button class="item" onclick={() => remove(s.id)}>
            <span class="dot" style:background={solid(s.color)}></span>{s.name}
          </button>
        {/each}
        <hr />
        <button class="item" onclick={() => (menuPage = 'main')}>Cancel</button>
      {/if}
    </Popover>
  </div>

  {#if !collapsed}
    <div class="rows" role="list">
      {#each tasks as task (task.id)}
        <TaskRow
          {task}
          {status}
          {statuses}
          onupdate={(patch) => onupdatetask(task, patch)}
          ondelete={() => ondeletetask(task)}
        />
      {/each}
      <form class="add" onsubmit={add}>
        <span class="plus" aria-hidden="true">＋</span>
        <input
          bind:value={draft}
          placeholder="Add task…  (!high @kat fri)"
          aria-label="Add task to {status.name}"
          onkeydown={(e) => e.key === 'Escape' && ((draft = ''), e.currentTarget.blur())}
        />
      </form>
    </div>
  {/if}
</section>

<style>
  section {
    margin-bottom: 18px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0 6px;
    border-bottom: 2px solid var(--hue);
  }

  .caret {
    transition: transform 0.15s ease;
  }

  .caret.collapsed {
    transform: rotate(-90deg);
  }

  h2 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
  }

  .rename {
    max-width: 220px;
    padding: 3px 8px;
  }

  .count {
    color: var(--text-muted);
    font-size: 0.8rem;
    margin-right: 2px;
  }

  .rows {
    display: flex;
    flex-direction: column;
  }

  .add {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px 4px 12px;
    border-bottom: 1px solid var(--border);
  }

  .add input {
    flex: 1;
    border: 0;
    background: transparent;
    padding: 6px 0;
    outline: none;
  }

  .plus {
    color: var(--text-muted);
    width: 18px;
    text-align: center;
  }

  .radio {
    width: 14px;
    color: var(--text-muted);
  }

  .note {
    margin: 4px 8px;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
</style>
