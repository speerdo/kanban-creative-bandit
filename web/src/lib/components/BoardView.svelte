<script lang="ts">
  import { autofocus } from '../actions';
  import type { Status } from '../api';
  import { ink, solid } from '../colors';
  import type { ProjectStore } from '../project.svelte';
  import { parseQuickAdd } from '../quickadd';
  import { sortable } from '../sortable';
  import { failed, labelIdsFor, people } from '../state.svelte';
  import Card from './Card.svelte';

  let { store }: { store: ProjectStore } = $props();

  /** Which column's add box is open, and its text. */
  let adding = $state<number | null>(null);
  let draft = $state('');

  async function add(e: SubmitEvent, status: Status) {
    e.preventDefault();
    const parsed = parseQuickAdd(draft, people.users);
    if (!parsed.title) return;
    const ok = await store.createTask({
      status_id: status.id,
      title: parsed.title,
      priority: parsed.priority,
      assignee_id: parsed.assignee?.id,
      due_date: parsed.due_date,
      label_ids: await labelIdsFor(parsed.labels).catch((e) => (failed(e), [])),
    });
    if (ok) draft = '';
  }

  function onmove(taskId: number, to: number, index: number) {
    const task = store.tasks.find((t) => t.id === taskId);
    if (task) store.moveTask(task, to, index);
  }
</script>

<div class="board">
  {#each store.statuses as status (status.id)}
    {@const tasks = store.byStatus.get(status.id) ?? []}
    <section class="column" style:--hue={solid(status.color)} aria-label={status.name}>
      <header>
        <h2 style:color={ink(status.color)}>{status.name}</h2>
        <span class="count mono">{tasks.length}</span>
        <button
          class="icon-btn"
          aria-label="Add task to {status.name}"
          onclick={() => {
            adding = status.id;
            draft = '';
          }}>＋</button
        >
      </header>

      <div class="cards" data-status={status.id} use:sortable={{ group: `tasks-${store.id}`, onmove }}>
        {#each tasks as task (task.id)}
          <Card {task} {store} />
        {/each}
      </div>

      {#if adding === status.id}
        <form class="add" onsubmit={(e) => add(e, status)}>
          <textarea
            bind:value={draft}
            rows="2"
            placeholder="Task name  !high @kat #label fri"
            aria-label="New task in {status.name}"
            use:autofocus
            onkeydown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                e.currentTarget.form?.requestSubmit();
              }
              if (e.key === 'Escape') adding = null;
            }}
            onblur={() => !draft.trim() && (adding = null)}
          ></textarea>
        </form>
      {:else}
        <button class="add-btn" onclick={() => ((adding = status.id), (draft = ''))}>＋ Add task</button>
      {/if}
    </section>
  {/each}
</div>

<style>
  .board {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 16px 24px 32px;
    overflow-x: auto;
    min-height: calc(100vh - 58px);
    scroll-snap-type: x proximity;
  }

  .column {
    flex: none;
    width: 288px;
    display: flex;
    flex-direction: column;
    max-height: calc(100vh - 90px);
    background: var(--surface-2);
    border-radius: var(--radius);
    border-top: 4px solid var(--hue);
    scroll-snap-align: start;
  }

  header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 8px 6px 12px;
  }

  h2 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
  }

  .count {
    color: var(--text-muted);
    font-size: 0.8rem;
    margin-right: auto;
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: calc(var(--card-pad) - 2px);
    padding: 4px 8px;
    overflow-y: auto;
    /* Empty columns still accept drops. */
    min-height: 40px;
  }

  .add,
  .add-btn {
    margin: 4px 8px 10px;
  }

  .add textarea {
    width: 100%;
    resize: none;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent);
    background: var(--surface);
  }

  .add-btn {
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    text-align: left;
  }

  .add-btn:hover {
    background: var(--hover);
    color: var(--text);
  }

  @media (max-width: 760px) {
    .board {
      padding: 12px 12px 24px;
      scroll-snap-type: x mandatory;
    }

    .column {
      width: min(300px, 84vw);
    }
  }
</style>
