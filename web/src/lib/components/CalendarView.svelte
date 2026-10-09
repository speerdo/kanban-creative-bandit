<!-- Calendar: tasks on their due dates (drag to reschedule, click a day to add), with my Google
     calendars drawn beside them, read-only. Google changes arrive by themselves; our tasks go
     to Google only through "Push to Google". -->
<script lang="ts">
  import Sortable from 'sortablejs';
  import { autofocus } from '../actions';
  import { api, type OverlayEvent, type Task } from '../api';
  import { addDays, eventDays, isoDate, monthGrid, startTime, weekOf } from '../calendar';
  import { solid } from '../colors';
  import { google, loadGoogle, push, refreshPending, since, syncNow } from '../google.svelte';
  import { listen } from '../live.svelte';
  import { parseQuickAdd } from '../quickadd';
  import { failed, labelIdsFor, openTask, people, session, userById, workspace } from '../state.svelte';
  import Avatar from './Avatar.svelte';
  import Popover from './Popover.svelte';

  type Mode = 'month' | 'week';

  const stored = (key: string, fallback: string) => {
    try {
      return localStorage.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  };
  const store = (key: string, value: string) => {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Private mode: the choice just isn't remembered.
    }
  };

  const today = isoDate(new Date());
  // Phones get the week, which stacks into an agenda.
  let mode = $state<Mode>(
    (stored('calendar.mode', matchMedia('(max-width: 760px)').matches ? 'week' : 'month') as Mode),
  );
  let anchor = $state(today);
  /** 'all', 'me', or a user id. */
  let who = $state<string>(stored('calendar.who', 'all'));
  let hiddenProjects = $state<number[]>(JSON.parse(stored('calendar.hidden', '[]')));
  let projectsOpen = $state(false);

  let tasks = $state<Task[]>([]);
  let events = $state<OverlayEvent[]>([]);
  let loading = $state(true);

  $effect(() => store('calendar.mode', mode));
  $effect(() => store('calendar.who', who));
  $effect(() => store('calendar.hidden', JSON.stringify(hiddenProjects)));

  const weeks = $derived(mode === 'month' ? monthGrid(anchor) : [weekOf(anchor)]);
  const from = $derived(weeks[0][0]);
  const to = $derived(weeks.at(-1)!.at(-1)!);
  const title = $derived.by(() => {
    if (mode === 'month') {
      const d = new Date(`${anchor.slice(0, 7)}-15T12:00:00`);
      return d.toLocaleDateString(undefined, { month: 'long', year: 'numeric' });
    }
    const fmt = (day: string, opts: Intl.DateTimeFormatOptions) =>
      new Date(`${day}T12:00:00`).toLocaleDateString(undefined, opts);
    const sameMonth = from.slice(0, 7) === to.slice(0, 7);
    return `${fmt(from, { month: 'short', day: 'numeric' })} – ${fmt(to, sameMonth ? { day: 'numeric', year: 'numeric' } : { month: 'short', day: 'numeric', year: 'numeric' })}`;
  });

  let loadSeq = 0;
  async function load() {
    const seq = ++loadSeq;
    try {
      const data = await api.calendar(from, to);
      if (seq !== loadSeq) return; // a newer range was asked for meanwhile
      tasks = data.tasks;
      events = data.events;
    } catch (e) {
      failed(e);
    } finally {
      if (seq === loadSeq) loading = false;
    }
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  const soon = () => {
    clearTimeout(timer);
    timer = setTimeout(load, 250);
  };

  $effect(() => {
    void from;
    void to;
    load();
  });

  $effect(() => {
    return listen({
      event(kind, data) {
        if (kind === 'calendar.synced') {
          if (data.user_id === session.me?.id) {
            soon();
            loadGoogle();
          }
          return;
        }
        if (!kind.startsWith('task.')) return;
        const due = data.due_date as string | null | undefined;
        if (tasks.some((t) => t.id === data.id) || (due && due >= from && due <= to)) soon();
        refreshPending();
      },
      resync: load,
    });
  });

  // Opening the calendar pulls from Google if the last pull is more than a few minutes old.
  $effect(() => {
    loadGoogle().then(() => {
      const s = google.status;
      if (!s?.connected) return;
      const age = s.last_sync_at ? Date.now() - new Date(s.last_sync_at).getTime() : Infinity;
      if (age > 5 * 60 * 1000) syncNow(true);
    });
  });

  // ---- what's shown --------------------------------------------------------------------------

  const visible = (t: Task) => {
    if (hiddenProjects.includes(t.project_id)) return false;
    if (who === 'me') return t.assignee_id === session.me?.id;
    if (who !== 'all') return t.assignee_id === Number(who);
    return true;
  };

  const tasksByDay = $derived.by(() => {
    const map = new Map<string, Task[]>();
    for (const t of tasks) {
      if (!t.due_date || !visible(t)) continue;
      if (!map.has(t.due_date)) map.set(t.due_date, []);
      map.get(t.due_date)!.push(t);
    }
    return map;
  });

  const eventsByDay = $derived.by(() => {
    const map = new Map<string, OverlayEvent[]>();
    for (const e of events) {
      const [first, last] = eventDays(e);
      for (let d = first < from ? from : first; d <= last && d <= to; d = addDays(d, 1)) {
        if (!map.has(d)) map.set(d, []);
        map.get(d)!.push(e);
      }
    }
    for (const list of map.values()) {
      list.sort((a, b) => Number(b.all_day) - Number(a.all_day) || a.start_at.localeCompare(b.start_at));
    }
    return map;
  });

  const projectOf = (t: Task) => workspace.projects.find((p) => p.id === t.project_id);
  const others = $derived(people.users.filter((u) => u.id !== session.me?.id));
  /** Items a month cell shows before "+n more". */
  const MONTH_LIMIT = 4;

  function move(step: number) {
    anchor =
      mode === 'week'
        ? addDays(anchor, 7 * step)
        : (() => {
            const [y, m] = anchor.split('-').map(Number);
            const d = new Date(y, m - 1 + step, 1);
            return isoDate(d);
          })();
  }

  function showWeek(day: string) {
    anchor = day;
    mode = 'week';
  }

  // ---- drag a task to another day -------------------------------------------------------------

  async function reschedule(taskId: number, day: string) {
    const task = tasks.find((t) => t.id === taskId);
    if (!task || task.due_date === day) return;
    const before = task.due_date;
    task.due_date = day;
    try {
      Object.assign(task, await api.updateTask(task.id, { due_date: day }));
      refreshPending();
    } catch (e) {
      task.due_date = before;
      failed(e);
    }
  }

  /** Each day's task list is a drop target; Svelte owns the DOM, so the move is undone and reported. */
  function dayDrop(node: HTMLElement, day: string) {
    let current = day;
    let anchorNode: Node | null = null;
    const instance = Sortable.create(node, {
      group: 'calendar',
      sort: false,
      draggable: '[data-task]',
      filter: '.no-drag',
      preventOnFilter: false,
      animation: 120,
      delay: 180,
      delayOnTouchOnly: true,
      touchStartThreshold: 4,
      ghostClass: 'drag-ghost',
      chosenClass: 'drag-chosen',
      dragClass: 'drag-active',
      fallbackOnBody: true,
      onStart(evt) {
        anchorNode = evt.item.nextSibling;
      },
      onEnd(evt) {
        const { item, from: src, to: dest } = evt;
        src.insertBefore(item, anchorNode);
        anchorNode = null;
        if (dest !== src) reschedule(Number(item.dataset.task), dest.dataset.day!);
      },
    });
    node.dataset.day = current;
    return {
      update(next: string) {
        current = next;
        node.dataset.day = next;
      },
      destroy() {
        instance.destroy();
      },
    };
  }

  // ---- add a task on a day ----------------------------------------------------------------

  let addingOn = $state<string | null>(null);
  let draft = $state('');
  let projectId = $state<number | null>(Number(stored('calendar.project', '0')) || null);
  const targetProject = $derived(
    workspace.projects.find((p) => p.id === projectId) ?? workspace.projects[0] ?? null,
  );

  function startAdding(day: string, e?: MouseEvent) {
    // Only clicks on the empty part of a day, not on its tasks or events.
    if (e && (e.target as HTMLElement).closest('[data-task], a, button, form')) return;
    if (!workspace.projects.length) return;
    addingOn = day;
    draft = '';
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const day = addingOn;
    const project = targetProject;
    if (!day || !project || !draft.trim()) return;
    const parsed = parseQuickAdd(draft, people.users);
    if (!parsed.title) return;
    try {
      const task = await api.createTask({
        project_id: project.id,
        title: parsed.title,
        priority: parsed.priority,
        // On my calendar, a new task is mine unless it says otherwise.
        assignee_id: parsed.assignee?.id ?? session.me?.id ?? null,
        due_date: parsed.due_date ?? day,
        label_ids: parsed.labels.length ? await labelIdsFor(parsed.labels) : undefined,
      });
      if (!tasks.some((t) => t.id === task.id)) tasks.push(task);
      store('calendar.project', String(project.id));
      draft = '';
      refreshPending();
    } catch (err) {
      failed(err);
    }
  }

  const tasksCalendar = $derived(google.status?.calendars.find((c) => c.role === 'tasks'));
</script>

<header class="head">
  <h1>Calendar</h1>
  <div class="nav">
    <button class="icon-btn" aria-label="Previous" title="Previous" onclick={() => move(-1)}>‹</button>
    <button class="btn today" onclick={() => (anchor = today)}>Today</button>
    <button class="icon-btn" aria-label="Next" title="Next" onclick={() => move(1)}>›</button>
    <h2 class="range">{title}</h2>
  </div>

  <div class="segmented" role="radiogroup" aria-label="Layout">
    {#each [['month', 'Month'], ['week', 'Week']] as [m, text] (m)}
      <button role="radio" aria-checked={mode === m} class:on={mode === m} onclick={() => (mode = m as Mode)}>{text}</button>
    {/each}
  </div>

  <select class="input who" bind:value={who} aria-label="Whose tasks">
    <option value="all">Everyone</option>
    <option value="me">Me</option>
    {#each others as u (u.id)}
      <option value={String(u.id)}>{u.display_name}</option>
    {/each}
  </select>

  <Popover bind:open={projectsOpen}>
    {#snippet trigger()}
      <button class="btn" aria-expanded={projectsOpen} onclick={() => (projectsOpen = !projectsOpen)}>
        Projects{hiddenProjects.length ? ` (${workspace.projects.length - hiddenProjects.filter((id) => workspace.projects.some((p) => p.id === id)).length})` : ''}
      </button>
    {/snippet}
    <div class="project-list">
      {#each workspace.projects as p (p.id)}
        <label>
          <input
            type="checkbox"
            checked={!hiddenProjects.includes(p.id)}
            onchange={(e) =>
              (hiddenProjects = e.currentTarget.checked
                ? hiddenProjects.filter((id) => id !== p.id)
                : [...hiddenProjects, p.id])}
          />
          <span class="dot" style:background={solid(p.color)}></span>
          {p.name}
        </label>
      {/each}
      {#if hiddenProjects.length}
        <button class="btn small" onclick={() => (hiddenProjects = [])}>Show all</button>
      {/if}
    </div>
  </Popover>

  <div class="google">
    {#if google.status?.connected}
      <span class="synced mono" title={google.status.last_error ?? undefined} class:error={!!google.status.last_error}>
        {google.status.last_error ? 'Sync problem' : `Synced ${since(google.status.last_sync_at)}`}
      </span>
      <button
        class="icon-btn"
        title="Sync now: pull changes from Google"
        aria-label="Sync now"
        disabled={google.syncing}
        class:spin={google.syncing}
        onclick={() => syncNow()}>↻</button
      >
      {#if tasksCalendar}
        <button
          class="btn primary"
          disabled={google.pushing || google.status.pending === 0}
          title="Put your dated tasks on your “{tasksCalendar.summary}” Google calendar"
          onclick={push}
        >
          {google.pushing ? 'Pushing…' : `Push to Google${google.status.pending ? ` (${google.status.pending})` : ''}`}
        </button>
      {:else}
        <a class="btn" href="#/settings">Choose a calendar</a>
      {/if}
    {:else if google.status?.configured}
      <a class="btn" href="#/settings">Connect Google</a>
    {/if}
  </div>
</header>

{#if google.status?.last_error}
  <p class="banner" role="alert">
    {google.status.last_error}
    <a href="#/settings">Settings</a>
  </p>
{/if}

<div class="cal" class:week={mode === 'week'} class:loading>
  <div class="dow" aria-hidden="true">
    {#each weeks[0] as d (d)}
      <span>{new Date(`${d}T12:00:00`).toLocaleDateString(undefined, { weekday: 'short' })}</span>
    {/each}
  </div>

  {#each weeks as week (week[0])}
    <div class="row">
      {#each week as day (day)}
        {@const dayTasks = tasksByDay.get(day) ?? []}
        {@const dayEvents = eventsByDay.get(day) ?? []}
        {@const total = dayTasks.length + dayEvents.length}
        {@const limit = mode === 'month' ? MONTH_LIMIT : Infinity}
        {@const shownTasks = dayTasks.slice(0, limit)}
        {@const shownEvents = dayEvents.slice(0, Math.max(0, limit - shownTasks.length))}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div
          class="day"
          class:other={mode === 'month' && day.slice(0, 7) !== anchor.slice(0, 7)}
          class:today={day === today}
          class:past={day < today}
          onclick={(e) => startAdding(day, e)}
        >
          <div class="day-head">
            <span class="num">
              {#if mode === 'week'}
                {new Date(`${day}T12:00:00`).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' })}
              {:else}
                {Number(day.slice(8))}
              {/if}
            </span>
            <button class="icon-btn add" aria-label="Add a task on {day}" title="Add a task" onclick={() => startAdding(day)}>＋</button>
          </div>

          <div class="tasks" use:dayDrop={day}>
            {#each shownTasks as t (t.id)}
              {@const p = projectOf(t)}
              <button
                class="task"
                class:done={!!t.completed_at}
                data-task={t.id}
                style:--project={solid(p?.color ?? 'slate')}
                title="{t.title}{p ? ` · ${p.name}` : ''}"
                onclick={() => openTask(t.id)}
              >
                <span class="t-title">{t.title}</span>
                {#if t.assignee_id}<Avatar user={userById(t.assignee_id)} size={16} />{/if}
              </button>
            {/each}
          </div>

          {#each shownEvents as e (e.google_event_id + e.calendar)}
            <a
              class="event"
              class:all-day={e.all_day}
              href={e.html_link ?? undefined}
              target="_blank"
              rel="noopener noreferrer"
              style:--cal={e.color ?? 'var(--c-slate)'}
              title="{e.summary} · {e.calendar}{e.location ? ` · ${e.location}` : ''}"
            >
              {#if startTime(e) && eventDays(e)[0] === day}<span class="time mono">{startTime(e)}</span>{/if}
              <span class="e-title">{e.summary}</span>
            </a>
          {/each}

          {#if total > shownTasks.length + shownEvents.length}
            <button class="more" onclick={() => showWeek(day)}>+{total - shownTasks.length - shownEvents.length} more</button>
          {/if}

          {#if addingOn === day}
            <form class="adder" onsubmit={add}>
              <input
                class="input"
                placeholder="Task title, then Enter"
                bind:value={draft}
                use:autofocus
                onkeydown={(e) => e.key === 'Escape' && (addingOn = null)}
                onblur={() => setTimeout(() => !draft.trim() && addingOn === day && (addingOn = null), 150)}
              />
              {#if workspace.projects.length > 1}
                <select class="input" aria-label="Project" value={targetProject?.id} onchange={(e) => (projectId = Number(e.currentTarget.value))}>
                  {#each workspace.projects as p (p.id)}
                    <option value={p.id}>{p.name}</option>
                  {/each}
                </select>
              {/if}
            </form>
          {/if}
        </div>
      {/each}
    </div>
  {/each}
</div>

{#if google.status?.connected && !tasksCalendar}
  <p class="hint">Choose where your tasks go in <a href="#/settings">Settings → Google</a> to use Push to Google.</p>
{:else if google.status?.configured && !google.status.connected}
  <p class="hint">Connect Google in <a href="#/settings">Settings</a> to see your Google calendars here.</p>
{/if}

<style>
  .head {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
    padding: 12px 24px;
    background: color-mix(in oklch, var(--surface) 88%, transparent);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 3px 0 var(--accent);
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
  }

  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .range {
    margin: 0 0 0 8px;
    font-size: 1rem;
    font-weight: 600;
    white-space: nowrap;
  }

  .today {
    padding-inline: 10px;
  }

  .segmented {
    display: inline-flex;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }

  .segmented button {
    border: 0;
    background: transparent;
    padding: 4px 12px;
    border-radius: 4px;
    color: var(--text-muted);
  }

  .segmented button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 1px 2px var(--shadow-2);
  }

  .who {
    width: auto;
  }

  .project-list {
    display: grid;
    gap: 2px;
    min-width: 200px;
    padding: 6px;
  }

  .project-list label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }

  .project-list label:hover {
    background: var(--hover);
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }

  .small {
    margin-top: 4px;
    font-size: 0.8rem;
  }

  .google {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .synced {
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .synced.error {
    color: var(--bad);
  }

  .spin {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .banner {
    margin: 12px 24px 0;
    padding: 8px 12px;
    border: 1px solid var(--bad);
    border-radius: var(--radius-sm);
    background: color-mix(in oklch, var(--bad) 10%, var(--surface));
    color: var(--text);
    font-size: 0.9rem;
  }

  .cal {
    margin: 12px 24px 24px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    transition: opacity 0.15s;
  }

  .cal.loading {
    opacity: 0.6;
  }

  .dow,
  .row {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
  }

  .dow span {
    padding: 6px 8px;
    font-family: var(--font-mono);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
  }

  .week .dow {
    display: none;
  }

  .day {
    position: relative;
    min-height: 118px;
    padding: 4px 5px 6px;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 2px;
    cursor: cell;
    min-width: 0;
  }

  .week .day {
    min-height: 60vh;
  }

  .row > .day:last-child {
    border-right: 0;
  }

  .row:last-child > .day {
    border-bottom: 0;
  }

  .day.other {
    background: var(--surface-2);
  }

  .day.other .num {
    color: var(--text-muted);
  }

  .day-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 22px;
  }

  .num {
    font-size: 0.8rem;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 999px;
  }

  .day.today .num {
    background: var(--accent);
    color: var(--accent-text);
  }

  .add {
    width: 22px;
    height: 22px;
    font-size: 0.9rem;
    opacity: 0;
  }

  .day:hover .add,
  .add:focus-visible {
    opacity: 1;
  }

  .tasks {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 6px;
  }

  .task,
  .event {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    min-width: 0;
    padding: 2px 5px;
    border-radius: 4px;
    font-size: 0.78rem;
    line-height: 1.35;
    text-align: left;
    text-decoration: none;
    cursor: pointer;
  }

  .task {
    border: 1px solid var(--border);
    border-left: 3px solid var(--project);
    background: var(--surface);
    color: var(--text);
    cursor: grab;
  }

  .task:hover {
    background: var(--hover);
  }

  .task.done .t-title {
    text-decoration: line-through;
    color: var(--text-muted);
  }

  .t-title,
  .e-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Google events: muted, read-only, in the calendar's own color. */
  .event {
    border: 0;
    color: var(--text-muted);
    background: transparent;
  }

  .event::before {
    content: '';
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--cal);
  }

  .event.all-day {
    background: color-mix(in oklch, var(--cal) 22%, var(--surface));
    color: var(--text);
  }

  .event.all-day::before {
    display: none;
  }

  .event:hover {
    background: var(--hover);
  }

  .time {
    flex: none;
    font-size: 0.7rem;
  }

  .more {
    align-self: flex-start;
    border: 0;
    background: transparent;
    padding: 1px 5px;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .more:hover {
    color: var(--text);
    text-decoration: underline;
  }

  .adder {
    display: grid;
    gap: 4px;
    margin-top: 2px;
  }

  .adder .input {
    font-size: 0.8rem;
    padding: 4px 6px;
  }

  .hint {
    margin: 0 24px 64px;
    font-size: 0.85rem;
    color: var(--on-canvas-muted);
  }

  .hint a,
  .banner a {
    color: inherit;
  }

  :global(.drag-ghost) {
    opacity: 0.4;
  }

  @media (max-width: 760px) {
    .head {
      padding-left: 52px;
    }

    .google {
      margin-left: 0;
      width: 100%;
    }

    .cal {
      margin: 8px 6px 24px;
    }

    .day {
      min-height: 84px;
      padding: 2px 2px 4px;
    }

    .task,
    .event {
      font-size: 0.7rem;
      padding: 1px 3px;
    }

    .task :global(.avatar),
    .time {
      display: none;
    }

    /* The week becomes an agenda: one day per row. */
    .week .row {
      grid-template-columns: 1fr;
    }

    .week .day {
      min-height: 64px;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }

    .week .task,
    .week .event {
      font-size: 0.85rem;
      padding: 4px 6px;
    }

    .add {
      opacity: 1;
    }
  }
</style>
