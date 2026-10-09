<script lang="ts">
  import { autofocus } from '../actions';
  import { PRIORITIES, type Priority, type Project, type Status, type Task, type TaskPatch } from '../api';
  import { ink, PRIORITY_COLOR, solid, tint } from '../colors';
  import { isoDate } from '../quickadd';
  import { openTask, people, selection, userById } from '../state.svelte';
  import Avatar from './Avatar.svelte';
  import LabelChips from './LabelChips.svelte';
  import Popover from './Popover.svelte';
  import StatusPill from './StatusPill.svelte';

  let {
    task,
    status,
    statuses,
    onupdate,
    ondelete,
    project,
    draggable = true,
  }: {
    task: Task;
    status: Status;
    statuses: Status[];
    onupdate: (patch: TaskPatch) => void;
    ondelete: () => void;
    /** Shown as a chip (My Tasks lists tasks from every project). */
    project?: Project;
    draggable?: boolean;
  } = $props();

  let editing = $state(false);
  let assigneeOpen = $state(false);
  let priorityOpen = $state(false);
  let menuOpen = $state(false);

  const done = $derived(task.completed_at !== null);
  const today = isoDate(new Date());
  const overdue = $derived(!done && task.due_date !== null && task.due_date < today);
  const assignee = $derived(userById(task.assignee_id));
  const priorityColor = $derived(PRIORITY_COLOR[task.priority]);

  function saveTitle(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value.trim();
    editing = false;
    if (value && value !== task.title) onupdate({ title: value });
  }

  function dueLabel(d: string): string {
    if (d === today) return 'Today';
    const date = new Date(`${d}T00:00`);
    const sameYear = date.getFullYear() === new Date().getFullYear();
    return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
  }

  const PRIORITY_LABEL: Record<Priority, string> = {
    none: 'No priority',
    low: 'Low',
    medium: 'Medium',
    high: 'High',
    urgent: 'Urgent',
  };
  const PRIORITY_ICON: Record<Priority, string> = { none: '–', low: '↓', medium: '=', high: '↑', urgent: '‼' };
</script>

<div
  class="row"
  class:done
  class:selected={selection.taskId === task.id}
  role="listitem"
  data-task={task.id}
  style:--stripe={priorityColor ? solid(priorityColor) : 'transparent'}
  onmouseenter={() => (selection.taskId = task.id)}
>
  {#if draggable}<span class="grip" title="Drag to move" aria-hidden="true">⠿</span>{:else}<span class="grip-space"></span>{/if}
  <input
    type="checkbox"
    class="check"
    checked={done}
    aria-label={done ? `Mark “${task.title}” not done` : `Complete “${task.title}”`}
    onchange={(e) => onupdate({ completed: e.currentTarget.checked })}
  />

  <div class="title">
    {#if editing}
      <input
        class="input"
        value={task.title}
        use:autofocus
        onblur={saveTitle}
        onkeydown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
          if (e.key === 'Escape') editing = false;
        }}
      />
    {:else}
      <button class="title-btn" onclick={() => openTask(task.id)}>{task.title}</button>
      {#if task.parent_task_id}<span class="sub-mark mono" title="Subtask">subtask</span>{/if}
      <LabelChips ids={task.label_ids} />
      {#if project}
        <span class="project-chip"><span class="pdot" style:background={solid(project.color)}></span>{project.name}</span>
      {/if}
      {#if task.subtask_count > 0}
        <span class="subs mono" title="Subtasks">☑ {task.subtasks_done}/{task.subtask_count}</span>
      {/if}
    {/if}
  </div>

  <div class="cells">
    <StatusPill {status} {statuses} onchange={(id) => onupdate({ status_id: id })} />

    <Popover bind:open={assigneeOpen} align="right">
      {#snippet trigger()}
        <button class="cell-btn" aria-label="Assignee: {assignee?.display_name ?? 'nobody'}" onclick={() => (assigneeOpen = !assigneeOpen)}>
          <Avatar user={assignee} />
        </button>
      {/snippet}
      {#each people.users as u (u.id)}
        <button class="item" onclick={() => ((assigneeOpen = false), onupdate({ assignee_id: u.id }))}>
          <Avatar user={u} size={18} />{u.display_name}
        </button>
      {/each}
      <hr />
      <button class="item" onclick={() => ((assigneeOpen = false), onupdate({ assignee_id: null }))}>
        <Avatar user={undefined} size={18} />Unassigned
      </button>
    </Popover>

    <label class="due" class:overdue class:empty={!task.due_date}>
      <span class="sr-only">Due date</span>
      <span aria-hidden="true">{task.due_date ? dueLabel(task.due_date) : 'Due'}</span>
      <input
        type="date"
        value={task.due_date ?? ''}
        onchange={(e) => onupdate({ due_date: e.currentTarget.value || null })}
      />
    </label>

    <Popover bind:open={priorityOpen} align="right">
      {#snippet trigger()}
        <button
          class="cell-btn prio"
          style:background={priorityColor ? tint(priorityColor, 18) : 'transparent'}
          style:color={priorityColor ? ink(priorityColor) : 'var(--text-muted)'}
          aria-label="Priority: {PRIORITY_LABEL[task.priority]}"
          title={PRIORITY_LABEL[task.priority]}
          onclick={() => (priorityOpen = !priorityOpen)}>{PRIORITY_ICON[task.priority]}</button
        >
      {/snippet}
      {#each PRIORITIES as p (p)}
        <button class="item" role="menuitemradio" aria-checked={task.priority === p} onclick={() => ((priorityOpen = false), onupdate({ priority: p }))}>
          <span class="picon" style:color={PRIORITY_COLOR[p] ? solid(PRIORITY_COLOR[p]!) : 'var(--text-muted)'}>{PRIORITY_ICON[p]}</span>
          {PRIORITY_LABEL[p]}
        </button>
      {/each}
    </Popover>

    <Popover bind:open={menuOpen} align="right">
      {#snippet trigger()}
        <button class="icon-btn more" aria-label="Task menu" onclick={() => (menuOpen = !menuOpen)}>⋯</button>
      {/snippet}
      <button class="item" onclick={() => ((menuOpen = false), openTask(task.id))}>Open details</button>
      <button class="item" onclick={() => ((menuOpen = false), (editing = true))}>Rename</button>
      <button class="item danger" onclick={() => ((menuOpen = false), ondelete())}>Delete task</button>
    </Popover>
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: var(--row-h);
    padding: 2px 4px 2px 0;
    border-bottom: 1px solid var(--border);
    box-shadow: inset 3px 0 0 var(--stripe);
    background: var(--surface);
  }

  .row:hover {
    background: var(--hover);
  }

  .row.selected {
    background: color-mix(in oklch, var(--accent) 8%, var(--surface));
  }

  .grip-space {
    width: 8px;
    flex: none;
  }

  .sub-mark {
    flex: none;
    font-size: 0.65rem;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .project-chip {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 1px 8px 1px 6px;
    border-radius: 999px;
    background: var(--surface-2);
    font-size: 0.75rem;
    color: var(--text-muted);
    max-width: 140px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .pdot {
    width: 7px;
    height: 7px;
    border-radius: 2px;
    flex: none;
  }

  .grip {
    flex: none;
    width: 14px;
    margin-left: 2px;
    margin-right: -6px;
    color: var(--text-muted);
    cursor: grab;
    opacity: 0;
    user-select: none;
    touch-action: none;
  }

  .row:hover .grip {
    opacity: 1;
  }

  .check {
    flex: none;
    width: 16px;
    height: 16px;
    accent-color: var(--ok);
    cursor: pointer;
  }

  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .title-btn {
    border: 0;
    background: none;
    padding: 4px 0;
    flex: 0 1 auto;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .done .title-btn {
    color: var(--text-muted);
    text-decoration: line-through;
  }

  .subs {
    flex: none;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .cells {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .cell-btn {
    display: inline-grid;
    place-items: center;
    min-width: 28px;
    height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
  }

  .cell-btn:hover {
    background: var(--surface-2);
  }

  .prio {
    font-weight: 700;
  }

  .picon {
    width: 14px;
    text-align: center;
    font-weight: 700;
  }

  .due {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 64px;
    height: 28px;
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
    cursor: pointer;
  }

  .due:hover {
    background: var(--surface-2);
  }

  .due.empty {
    color: var(--text-muted);
    opacity: 0;
  }

  .row:hover .due.empty,
  .due.empty:focus-within {
    opacity: 1;
  }

  .due.overdue {
    color: var(--bad);
    font-weight: 600;
  }

  /* The native picker sits invisibly on top so a tap opens it on every platform. */
  .due input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .due input::-webkit-calendar-picker-indicator {
    position: absolute;
    inset: 0;
    width: auto;
    height: auto;
    cursor: pointer;
  }

  .more {
    opacity: 0;
  }

  .row:hover .more,
  .more:focus-visible {
    opacity: 1;
  }

  :global(.row.drag-ghost) {
    opacity: 0.4;
    background: var(--surface-2);
  }

  @media (max-width: 760px) {
    .row {
      flex-wrap: wrap;
      padding: 6px 4px 6px 10px;
    }

    .title {
      flex-basis: calc(100% - 60px);
    }

    .cells {
      width: 100%;
      padding-left: 38px;
    }

    .due.empty,
    .more {
      opacity: 1;
    }
  }

  @media (hover: none) {
    .due.empty,
    .more,
    .grip {
      opacity: 1;
    }
  }
</style>
