<!-- Everything assigned to me, across projects, grouped by when it's due. -->
<script lang="ts">
  import { api, type Status, type Task, type TaskPatch } from '../api';
  import { listen } from '../live.svelte';
  import { isoDate } from '../quickadd';
  import { failed, session, toast, workspace } from '../state.svelte';
  import TaskRow from './TaskRow.svelte';

  let tasks = $state<Task[]>([]);
  let statuses = $state<Status[]>([]);
  let loading = $state(true);
  let showDone = $state(false);

  async function load() {
    try {
      const [t, s] = await Promise.all([
        api.tasks({ assignee: 'me', any_level: true, completed: showDone ? undefined : false }),
        api.allStatuses(),
      ]);
      tasks = t;
      statuses = s;
    } catch (e) {
      failed(e);
    } finally {
      loading = false;
    }
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  const soon = () => {
    clearTimeout(timer);
    timer = setTimeout(load, 300);
  };

  $effect(() => {
    void showDone;
    load();
    return listen({
      event(kind, data) {
        // Anything that might add, remove or change one of my tasks.
        if (!kind.startsWith('task.') && !kind.startsWith('status.')) return;
        if (data.assignee_id === session.me?.id || tasks.some((t) => t.id === data.id) || kind.startsWith('status.')) soon();
      },
      resync: load,
    });
  });

  async function update(task: Task, patch: TaskPatch) {
    const before = $state.snapshot(task);
    if (patch.completed !== undefined) task.completed_at = patch.completed ? new Date().toISOString() : null;
    try {
      Object.assign(task, await api.updateTask(task.id, patch));
      if (task.assignee_id !== session.me?.id) {
        tasks = tasks.filter((t) => t.id !== task.id);
        toast(`“${task.title}” is no longer assigned to you`);
      }
    } catch (e) {
      Object.assign(task, before);
      failed(e);
    }
  }

  async function remove(task: Task) {
    tasks = tasks.filter((t) => t.id !== task.id);
    await api.deleteTask(task.id).catch(failed);
  }

  // ---- grouping --------------------------------------------------------------------------------

  const today = new Date();
  const todayIso = isoDate(today);
  const endOfWeek = (() => {
    const d = new Date(today);
    d.setDate(d.getDate() + ((7 - d.getDay()) % 7)); // through Sunday
    return isoDate(d);
  })();

  type Group = { key: string; title: string; tasks: Task[] };
  const groups = $derived.by((): Group[] => {
    const g: Group[] = [
      { key: 'overdue', title: 'Overdue', tasks: [] },
      { key: 'today', title: 'Today', tasks: [] },
      { key: 'week', title: 'This week', tasks: [] },
      { key: 'later', title: 'Later', tasks: [] },
      { key: 'none', title: 'No date', tasks: [] },
    ];
    for (const t of tasks) {
      const d = t.due_date;
      const i = !d ? 4 : d < todayIso && !t.completed_at ? 0 : d <= todayIso ? 1 : d <= endOfWeek ? 2 : 3;
      g[i].tasks.push(t);
    }
    for (const x of g) x.tasks.sort((a, b) => (a.due_date ?? '9').localeCompare(b.due_date ?? '9') || a.id - b.id);
    return g.filter((x) => x.tasks.length > 0);
  });

  const statusOf = (t: Task) => statuses.find((s) => s.id === t.status_id);
  const statusesOf = (t: Task) => statuses.filter((s) => s.project_id === t.project_id);
  const projectOf = (t: Task) => workspace.projects.find((p) => p.id === t.project_id);
</script>

<header class="head">
  <h1>My tasks</h1>
  <label class="toggle"><input type="checkbox" bind:checked={showDone} /> Show completed</label>
</header>

<div class="list">
  {#if loading}
    <p class="muted">Loading…</p>
  {:else if groups.length === 0}
    <div class="empty">
      <p><strong>Nothing assigned to you.</strong></p>
      <p class="muted">Tasks assigned to you in any project show up here. Try <kbd>Q</kbd> then <code>@{session.me?.username}</code>.</p>
    </div>
  {:else}
    {#each groups as g (g.key)}
      <section>
        <h2 class:overdue={g.key === 'overdue'}>{g.title} <span class="count mono">{g.tasks.length}</span></h2>
        <div role="list">
          {#each g.tasks as task (task.id)}
            {@const status = statusOf(task)}
            {#if status}
              <TaskRow
                {task}
                {status}
                statuses={statusesOf(task)}
                project={projectOf(task)}
                draggable={false}
                onupdate={(p) => update(task, p)}
                ondelete={() => remove(task)}
              />
            {/if}
          {/each}
        </div>
      </section>
    {/each}
  {/if}
</div>

<style>
  .head {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 24px;
    background: color-mix(in oklch, var(--surface) 88%, transparent);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 3px 0 var(--accent);
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
  }

  .toggle {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .list {
    margin: 16px 24px 96px;
    padding: 12px 16px 4px;
    max-width: 1100px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  section {
    margin-bottom: 18px;
  }

  h2 {
    margin: 4px 0 6px;
    padding-bottom: 6px;
    font-size: 0.95rem;
    border-bottom: 2px solid var(--border);
  }

  h2.overdue {
    color: var(--bad);
    border-color: var(--bad);
  }

  .count {
    font-size: 0.8rem;
    font-weight: 400;
    color: var(--text-muted);
  }

  .empty {
    padding: 32px 8px;
    text-align: center;
  }

  kbd {
    font-family: var(--font-mono);
    font-size: 0.8em;
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 4px;
  }

  @media (max-width: 760px) {
    .head {
      padding-left: 52px;
    }

    .list {
      margin: 8px 6px 96px;
      padding: 8px 8px 2px;
    }
  }
</style>
