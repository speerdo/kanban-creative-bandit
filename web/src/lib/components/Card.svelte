<!-- A task on the board. Drag the card anywhere except its buttons. -->
<script lang="ts">
  import { autofocus } from '../actions';
  import type { Task } from '../api';
  import { ink, PRIORITY_COLOR, solid, tint } from '../colors';
  import type { ProjectStore } from '../project.svelte';
  import { isoDate } from '../quickadd';
  import { openTask, selection, userById } from '../state.svelte';
  import Avatar from './Avatar.svelte';
  import LabelChips from './LabelChips.svelte';
  import Popover from './Popover.svelte';

  let { task, store }: { task: Task; store: ProjectStore } = $props();

  let menuOpen = $state(false);
  let renaming = $state(false);

  const done = $derived(task.completed_at !== null);
  const today = isoDate(new Date());
  const overdue = $derived(!done && task.due_date !== null && task.due_date < today);
  const priorityColor = $derived(PRIORITY_COLOR[task.priority]);
  const assignee = $derived(userById(task.assignee_id));

  function dueLabel(d: string): string {
    if (d === today) return 'Today';
    const date = new Date(`${d}T00:00`);
    const sameYear = date.getFullYear() === new Date().getFullYear();
    return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
  }

  function rename(e: Event) {
    const value = (e.currentTarget as HTMLTextAreaElement).value.trim();
    renaming = false;
    if (value && value !== task.title) store.updateTask(task, { title: value });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (Enter opens it via the keyboard shortcuts) -->
<article
  class="card"
  class:done
  class:selected={selection.taskId === task.id}
  data-task={task.id}
  style:--stripe={priorityColor ? solid(priorityColor) : 'transparent'}
  onmouseenter={() => (selection.taskId = task.id)}
  onclick={(e) => {
    // Buttons and the rename box handle their own clicks.
    if (!(e.target as HTMLElement).closest('button, textarea, input, .panel')) openTask(task.id);
  }}
>
  <div class="top">
    <button
      class="check"
      class:on={done}
      aria-label={done ? `Mark “${task.title}” not done` : `Complete “${task.title}”`}
      onclick={() => store.updateTask(task, { completed: !done })}>{done ? '✓' : ''}</button
    >
    {#if renaming}
      <textarea
        class="no-drag"
        rows="2"
        value={task.title}
        use:autofocus
        onblur={rename}
        onkeydown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            e.currentTarget.blur();
          }
          if (e.key === 'Escape') renaming = false;
        }}
      ></textarea>
    {:else}
      <p class="title">{task.title}</p>
    {/if}
    <Popover bind:open={menuOpen} align="right">
      {#snippet trigger()}
        <button class="icon-btn more" aria-label="Task menu" onclick={() => (menuOpen = !menuOpen)}>⋯</button>
      {/snippet}
      <button class="item" onclick={() => ((menuOpen = false), (renaming = true))}>Rename</button>
      <button class="item danger" onclick={() => ((menuOpen = false), store.deleteTask(task))}>Delete task</button>
    </Popover>
  </div>

  {#if task.label_ids.length}
    <div class="labels"><LabelChips ids={task.label_ids} max={4} /></div>
  {/if}

  {#if task.due_date || task.priority !== 'none' || assignee || task.subtask_count > 0}
    <div class="meta">
      {#if task.priority !== 'none' && priorityColor}
        <span class="chip" style:background={tint(priorityColor, 18)} style:color={ink(priorityColor)}>{task.priority}</span>
      {/if}
      {#if task.due_date}
        <span class="due" class:overdue>{dueLabel(task.due_date)}</span>
      {/if}
      {#if task.subtask_count > 0}
        <span class="subs mono" title="Subtasks">☑ {task.subtasks_done}/{task.subtask_count}</span>
      {/if}
      <span class="spacer"></span>
      {#if assignee}<Avatar user={assignee} size={22} />{/if}
    </div>
  {/if}
</article>

<style>
  .card {
    position: relative;
    padding: var(--card-pad) 8px var(--card-pad) 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: inset 3px 0 0 var(--stripe);
    cursor: grab;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }

  .card:hover {
    border-color: color-mix(in oklch, var(--text-muted) 45%, var(--border));
  }

  .card.selected {
    border-color: var(--accent);
  }

  .labels {
    margin-top: 6px;
    padding-left: 26px;
  }

  .top {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  .check {
    flex: none;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    margin-top: 1px;
    padding: 0;
    border-radius: 50%;
    border: 1.5px solid var(--text-muted);
    background: transparent;
    color: #fff;
    font-size: 11px;
    font-weight: 700;
  }

  .check:hover {
    border-color: var(--ok);
  }

  .check.on {
    background: var(--ok);
    border-color: var(--ok);
  }

  .title {
    flex: 1;
    margin: 0;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }

  .done .title {
    color: var(--text-muted);
    text-decoration: line-through;
  }

  textarea {
    flex: 1;
    resize: none;
    padding: 2px 4px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--surface);
    user-select: text;
  }

  .more {
    margin: -4px -2px 0 0;
    width: 24px;
    height: 24px;
    opacity: 0;
  }

  .card:hover .more,
  .more:focus-visible {
    opacity: 1;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    padding-left: 26px;
    font-size: 0.8rem;
  }

  .chip {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: capitalize;
  }

  .due {
    color: var(--text-muted);
  }

  .due.overdue {
    color: var(--bad);
    font-weight: 600;
  }

  .subs {
    color: var(--text-muted);
    font-size: 0.75rem;
  }

  .spacer {
    flex: 1;
  }

  @media (hover: none) {
    .more {
      opacity: 1;
    }
  }

  /* SortableJS states (classes are added outside Svelte, hence :global). */
  :global(.card.drag-ghost) {
    opacity: 0.35;
  }

  :global(.card.drag-active) {
    transform: rotate(2deg);
    box-shadow: var(--shadow);
  }
</style>
