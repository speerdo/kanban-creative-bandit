<!-- The task detail slide-over (Asana-style): the list or board stays visible beside it. -->
<script lang="ts">
  import { autofocus } from '../actions';
  import { api, PRIORITIES, type Activity, type Comment, type Priority, type Status, type Task, type TaskPatch } from '../api';
  import { ink, PRIORITY_COLOR, solid, tint } from '../colors';
  import { listen } from '../live.svelte';
  import { active } from '../project.svelte';
  import { isoDate } from '../quickadd';
  import { failed, openTask, people, session, toast, userById, workspace } from '../state.svelte';
  import Avatar from './Avatar.svelte';
  import LabelChips from './LabelChips.svelte';
  import LabelPicker from './LabelPicker.svelte';
  import Markdown from './Markdown.svelte';
  import Popover from './Popover.svelte';
  import StatusPill from './StatusPill.svelte';

  let { taskId }: { taskId: number } = $props();

  // When the task is in the open project, edit the store's copy so the list/board updates too.
  let local = $state<Task | null>(null);
  const storeTask = $derived(active.store?.tasks.find((t) => t.id === taskId));
  const task = $derived(storeTask ?? local);

  let statuses = $state<Status[]>([]);
  let subtasks = $state<Task[]>([]);
  let comments = $state<Comment[]>([]);
  let activity = $state<Activity[]>([]);
  let missing = $state(false);

  const project = $derived(workspace.projects.find((p) => p.id === task?.project_id));
  const status = $derived(statuses.find((s) => s.id === task?.status_id));
  const parent = $derived(task?.parent_task_id ? (active.store?.tasks.find((t) => t.id === task!.parent_task_id) ?? null) : null);

  async function load() {
    try {
      const t = await api.task(taskId);
      local = t;
      missing = false;
      const [s, subs, c, a] = await Promise.all([
        active.store?.id === t.project_id ? Promise.resolve(active.store.statuses) : api.statuses(t.project_id),
        api.tasks({ parent: t.id }),
        api.comments(t.id),
        api.activity(t.id),
      ]);
      statuses = s;
      subtasks = subs.sort((x, y) => (x.position < y.position ? -1 : 1));
      comments = c;
      activity = a;
    } catch (e) {
      missing = true;
      failed(e);
    }
  }

  let activityTimer: ReturnType<typeof setTimeout> | undefined;
  function refreshActivity() {
    clearTimeout(activityTimer);
    activityTimer = setTimeout(async () => {
      activity = await api.activity(taskId).catch(() => activity);
    }, 250);
  }

  $effect(() => {
    void taskId;
    load();
    return listen({
      event(kind, data) {
        if (kind.startsWith('comment.') && data.task_id === taskId) {
          if (kind === 'comment.deleted') comments = comments.filter((c) => c.id !== data.id);
          else upsert(comments, data as unknown as Comment);
          return;
        }
        if (!kind.startsWith('task.')) return;
        if (data.id === taskId) {
          if (kind === 'task.deleted') {
            toast('This task was deleted');
            openTask(null);
          } else {
            if (local) Object.assign(local, data);
            refreshActivity();
          }
        } else if (data.parent_task_id === taskId) {
          if (kind === 'task.deleted') subtasks = subtasks.filter((s) => s.id !== data.id);
          else upsert(subtasks, data as unknown as Task);
        }
      },
      resync: load,
    });
  });

  function upsert<T extends { id: number }>(list: T[], item: T) {
    const existing = list.find((x) => x.id === item.id);
    if (existing) Object.assign(existing, item);
    else list.push(item);
  }

  async function patch(p: TaskPatch) {
    if (!task) return;
    if (storeTask && active.store) {
      await active.store.updateTask(storeTask, p);
    } else if (local) {
      const before = $state.snapshot(local);
      Object.assign(local, p);
      try {
        Object.assign(local, await api.updateTask(local.id, p));
      } catch (e) {
        Object.assign(local, before);
        failed(e);
      }
    }
    refreshActivity();
  }

  // ---- title & description ---------------------------------------------------------------------

  function saveTitle(e: Event) {
    const el = e.currentTarget as HTMLTextAreaElement;
    const value = el.value.replace(/\s+/g, ' ').trim();
    if (!value) el.value = task?.title ?? '';
    else if (value !== task?.title) patch({ title: value });
  }

  let editingDesc = $state(false);
  let descDraft = $state('');

  function startDesc() {
    descDraft = task?.description ?? '';
    editingDesc = true;
  }

  function saveDesc() {
    editingDesc = false;
    if (descDraft.trim() !== (task?.description ?? '')) patch({ description: descDraft });
  }

  // ---- subtasks --------------------------------------------------------------------------------

  let subDraft = $state('');

  async function addSubtask(e: SubmitEvent) {
    e.preventDefault();
    const title = subDraft.trim();
    if (!title || !task) return;
    try {
      upsert(subtasks, await api.createTask({ parent_task_id: task.id, title }));
      subDraft = '';
    } catch (err) {
      failed(err);
    }
  }

  async function toggleSubtask(sub: Task) {
    const before = $state.snapshot(sub);
    sub.completed_at = sub.completed_at ? null : new Date().toISOString();
    try {
      Object.assign(sub, await api.updateTask(sub.id, { completed: !before.completed_at }));
    } catch (e) {
      Object.assign(sub, before);
      failed(e);
    }
  }

  async function deleteSubtask(sub: Task) {
    subtasks = subtasks.filter((s) => s.id !== sub.id);
    await api.deleteTask(sub.id).catch(failed);
  }

  // ---- comments --------------------------------------------------------------------------------

  let draft = $state('');
  let sending = $state(false);
  let editingComment = $state<number | null>(null);
  let editDraft = $state('');

  async function send() {
    if (!draft.trim() || sending) return;
    sending = true;
    try {
      upsert(comments, await api.createComment(taskId, draft));
      draft = '';
    } catch (e) {
      failed(e);
    } finally {
      sending = false;
    }
  }

  async function saveComment(c: Comment) {
    editingComment = null;
    if (editDraft.trim() === c.body) return;
    try {
      Object.assign(c, await api.updateComment(c.id, editDraft));
    } catch (e) {
      failed(e);
    }
  }

  async function deleteComment(c: Comment) {
    if (!confirm('Delete this comment?')) return;
    comments = comments.filter((x) => x.id !== c.id);
    await api.deleteComment(c.id).catch(failed);
  }

  // ---- feed (comments + history, oldest first) -------------------------------------------------

  type Item = { at: string; key: string } & ({ comment: Comment } | { event: Activity });
  const feed = $derived(
    [
      ...comments.map((c): Item => ({ at: c.created_at, key: `c${c.id}`, comment: c })),
      ...activity.map((a): Item => ({ at: a.created_at, key: `a${a.id}`, event: a })),
    ].sort((x, y) => (x.at < y.at ? -1 : x.at > y.at ? 1 : x.key < y.key ? -1 : 1)),
  );

  function describe(a: Activity): string {
    const q = (v: string | null) => `“${v}”`;
    switch (a.kind) {
      case 'created':
        return 'created this task';
      case 'status':
        return `moved this from ${a.from_value ?? '?'} to ${a.to_value}`;
      case 'assignee':
        return a.to_value ? `assigned this to ${a.to_value}` : 'unassigned this';
      case 'priority':
        return a.to_value === 'none' ? 'cleared the priority' : `set priority to ${a.to_value}`;
      case 'due':
        return a.to_value ? `set the due date to ${day(a.to_value)}` : 'removed the due date';
      case 'start':
        return a.to_value ? `set the start date to ${day(a.to_value)}` : 'removed the start date';
      case 'title':
        return `renamed this from ${q(a.from_value)}`;
      case 'description':
        return 'edited the description';
      case 'labels':
        return a.to_value ? `changed labels to ${a.to_value}` : 'removed all labels';
      case 'completed':
        return 'marked this complete';
      case 'reopened':
        return 'reopened this';
      default:
        return a.kind;
    }
  }

  const day = (d: string) => new Date(`${d}T00:00`).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });

  function ago(iso: string): string {
    const s = (Date.now() - new Date(iso).getTime()) / 1000;
    if (s < 60) return 'just now';
    if (s < 3600) return `${Math.floor(s / 60)}m ago`;
    if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
    if (s < 7 * 86400) return `${Math.floor(s / 86400)}d ago`;
    return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }

  // ---- field popovers --------------------------------------------------------------------------

  let assigneeOpen = $state(false);
  let priorityOpen = $state(false);
  let labelsOpen = $state(false);
  let menuOpen = $state(false);

  const PRIORITY_LABEL: Record<Priority, string> = { none: 'None', low: 'Low', medium: 'Medium', high: 'High', urgent: 'Urgent' };
  const today = isoDate(new Date());
  const overdue = $derived(!!task && !task.completed_at && !!task.due_date && task.due_date < today);

  async function remove() {
    if (!task || !confirm(`Delete “${task.title}”${task.subtask_count ? ' and its subtasks' : ''}?`)) return;
    menuOpen = false;
    try {
      if (storeTask && active.store) await active.store.deleteTask(storeTask);
      else await api.deleteTask(task.id);
      openTask(null);
    } catch (e) {
      failed(e);
    }
  }
</script>

<aside class="panel" aria-label="Task details">
  <header>
    {#if project}
      <span class="crumb"><span class="dot" style:background={solid(project.color)}></span>{project.name}</span>
    {/if}
    {#if parent}
      <button class="crumb link" onclick={() => openTask(parent.id)}>› {parent.title}</button>
    {:else if task?.parent_task_id}
      <button class="crumb link" onclick={() => openTask(task!.parent_task_id)}>› parent task</button>
    {/if}
    <span class="spacer"></span>
    {#if task}
      <Popover bind:open={menuOpen} align="right">
        {#snippet trigger()}
          <button class="icon-btn" aria-label="Task menu" onclick={() => (menuOpen = !menuOpen)}>⋯</button>
        {/snippet}
        <button class="item danger" onclick={remove}>Delete task</button>
      </Popover>
    {/if}
    <button class="icon-btn" aria-label="Close (Esc)" title="Close (Esc)" onclick={() => openTask(null)}>✕</button>
  </header>

  {#if missing && !task}
    <p class="muted pad">This task doesn't exist any more.</p>
  {:else if !task}
    <p class="muted pad">Loading…</p>
  {:else}
    <div class="body">
      <div class="title-row">
        <button
          class="check"
          class:on={task.completed_at}
          aria-label={task.completed_at ? 'Mark not done' : 'Mark complete'}
          onclick={() => patch({ completed: !task!.completed_at })}>{task.completed_at ? '✓' : ''}</button
        >
        {#key task.title}
          <textarea
            class="title"
            rows="1"
            value={task.title}
            aria-label="Title"
            oninput={(e) => {
              const el = e.currentTarget;
              el.style.height = 'auto';
              el.style.height = `${el.scrollHeight}px`;
            }}
            onkeydown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                e.currentTarget.blur();
              }
            }}
            onblur={saveTitle}
          ></textarea>
        {/key}
      </div>

      <dl class="fields">
        <dt>Status</dt>
        <dd>
          {#if status}
            <StatusPill {status} {statuses} onchange={(id) => patch({ status_id: id })} />
          {/if}
        </dd>

        <dt>Assignee</dt>
        <dd>
          <Popover bind:open={assigneeOpen}>
            {#snippet trigger()}
              <button class="value" onclick={() => (assigneeOpen = !assigneeOpen)}>
                <Avatar user={userById(task!.assignee_id)} size={20} />
                {userById(task!.assignee_id)?.display_name ?? 'Unassigned'}
              </button>
            {/snippet}
            {#each people.users as u (u.id)}
              <button class="item" onclick={() => ((assigneeOpen = false), patch({ assignee_id: u.id }))}>
                <Avatar user={u} size={18} />{u.display_name}
              </button>
            {/each}
            <hr />
            <button class="item" onclick={() => ((assigneeOpen = false), patch({ assignee_id: null }))}>Unassigned</button>
          </Popover>
        </dd>

        <dt>Due date</dt>
        <dd>
          <input
            class="date"
            class:overdue
            type="date"
            value={task.due_date ?? ''}
            onchange={(e) => patch({ due_date: e.currentTarget.value || null })}
          />
        </dd>

        <dt>Start date</dt>
        <dd>
          <input class="date" type="date" value={task.start_date ?? ''} onchange={(e) => patch({ start_date: e.currentTarget.value || null })} />
        </dd>

        <dt>Priority</dt>
        <dd>
          <Popover bind:open={priorityOpen}>
            {#snippet trigger()}
              {@const c = PRIORITY_COLOR[task!.priority]}
              <button
                class="value"
                style:background={c ? tint(c, 18) : undefined}
                style:color={c ? ink(c) : undefined}
                onclick={() => (priorityOpen = !priorityOpen)}>{PRIORITY_LABEL[task!.priority]}</button
              >
            {/snippet}
            {#each PRIORITIES as p (p)}
              <button class="item" role="menuitemradio" aria-checked={task.priority === p} onclick={() => ((priorityOpen = false), patch({ priority: p }))}>
                <span class="dot" style:background={PRIORITY_COLOR[p] ? solid(PRIORITY_COLOR[p]!) : 'transparent'}></span>{PRIORITY_LABEL[p]}
              </button>
            {/each}
          </Popover>
        </dd>

        <dt>Labels</dt>
        <dd>
          <Popover bind:open={labelsOpen}>
            {#snippet trigger()}
              <button class="value" onclick={() => (labelsOpen = !labelsOpen)}>
                {#if task!.label_ids.length}<LabelChips ids={task!.label_ids} max={8} />{:else}<span class="muted">Add labels</span>{/if}
              </button>
            {/snippet}
            <LabelPicker selected={task.label_ids} onchange={(ids) => patch({ label_ids: ids })} />
          </Popover>
        </dd>
      </dl>

      <section>
        <h3>Description</h3>
        {#if editingDesc}
          <textarea
            class="input desc"
            bind:value={descDraft}
            rows="6"
            use:autofocus
            placeholder="Markdown works: **bold**, - lists, - [ ] checklists, links…"
            onblur={saveDesc}
            onkeydown={(e) => {
              if (e.key === 'Escape') {
                e.stopPropagation();
                editingDesc = false;
              }
              if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) e.currentTarget.blur();
            }}
          ></textarea>
        {:else}
          <button class="desc-view" onclick={startDesc} aria-label="Edit description">
            {#if task.description}
              <Markdown text={task.description} />
            {:else}
              <span class="muted">Add a description…</span>
            {/if}
          </button>
        {/if}
      </section>

      <section>
        <h3>Subtasks {#if subtasks.length}<span class="count mono">{subtasks.filter((s) => s.completed_at).length}/{subtasks.length}</span>{/if}</h3>
        <ul class="subtasks">
          {#each subtasks as sub (sub.id)}
            <li class:done={sub.completed_at}>
              <input type="checkbox" checked={!!sub.completed_at} aria-label="Complete {sub.title}" onchange={() => toggleSubtask(sub)} />
              <button class="sub-title" onclick={() => openTask(sub.id)}>{sub.title}</button>
              {#if sub.assignee_id}<Avatar user={userById(sub.assignee_id)} size={18} />{/if}
              <button class="icon-btn del" aria-label="Delete subtask {sub.title}" onclick={() => deleteSubtask(sub)}>✕</button>
            </li>
          {/each}
        </ul>
        <form onsubmit={addSubtask}>
          <input class="input" bind:value={subDraft} placeholder="＋ Add subtask" aria-label="Add subtask" />
        </form>
      </section>

      <section>
        <h3>Comments &amp; activity</h3>
        <ol class="feed">
          {#each feed as item (item.key)}
            {#if 'comment' in item}
              {@const c = item.comment}
              <li class="comment">
                <Avatar user={userById(c.author_id)} size={26} />
                <div class="bubble">
                  <div class="meta">
                    <strong>{userById(c.author_id)?.display_name ?? 'Someone'}</strong>
                    <span class="muted" title={new Date(c.created_at).toLocaleString()}>{ago(c.created_at)}{c.edited_at ? ' · edited' : ''}</span>
                    {#if c.author_id === session.me?.id && editingComment !== c.id}
                      <span class="spacer"></span>
                      <button class="link" onclick={() => ((editingComment = c.id), (editDraft = c.body))}>Edit</button>
                      <button class="link danger" onclick={() => deleteComment(c)}>Delete</button>
                    {/if}
                  </div>
                  {#if editingComment === c.id}
                    <textarea
                      class="input"
                      rows="3"
                      bind:value={editDraft}
                      use:autofocus
                      onkeydown={(e) => {
                        if (e.key === 'Escape') {
                          e.stopPropagation();
                          editingComment = null;
                        }
                        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) saveComment(c);
                      }}
                      onblur={() => saveComment(c)}
                    ></textarea>
                  {:else}
                    <Markdown text={c.body} />
                  {/if}
                </div>
              </li>
            {:else}
              {@const a = item.event}
              <li class="event">
                <span class="tick" aria-hidden="true"></span>
                <span><strong>{userById(a.actor_id)?.display_name ?? 'Someone'}</strong> {describe(a)}</span>
                <span class="muted when" title={new Date(a.created_at).toLocaleString()}>{ago(a.created_at)}</span>
              </li>
            {/if}
          {/each}
        </ol>
      </section>
    </div>

    <form
      class="composer"
      onsubmit={(e) => {
        e.preventDefault();
        send();
      }}
    >
      <Avatar user={session.me ?? undefined} size={26} />
      <textarea
        class="input"
        rows="2"
        bind:value={draft}
        placeholder="Add a comment…  (Ctrl+Enter to send)"
        aria-label="Comment"
        onkeydown={(e) => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            send();
          }
        }}
      ></textarea>
      <button class="btn primary" type="submit" disabled={!draft.trim() || sending}>Send</button>
    </form>
  {/if}
</aside>

<style>
  .panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 50;
    width: min(540px, 100vw);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--border);
    box-shadow: var(--shadow);
    animation: slide-in 0.16s ease-out;
  }

  @keyframes slide-in {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }

  header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px 10px 20px;
    border-bottom: 1px solid var(--border);
    font-size: 0.85rem;
  }

  .crumb {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button.crumb {
    border: 0;
    background: none;
    padding: 0;
  }

  button.crumb:hover {
    color: var(--text);
    text-decoration: underline;
  }

  .spacer {
    flex: 1;
  }

  .pad {
    padding: 20px;
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px 24px;
  }

  .title-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-bottom: 14px;
  }

  .check {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-top: 5px;
    padding: 0;
    border-radius: 50%;
    border: 1.5px solid var(--text-muted);
    background: transparent;
    color: #fff;
    font-size: 13px;
    font-weight: 700;
  }

  .check.on {
    background: var(--ok);
    border-color: var(--ok);
  }

  .title {
    flex: 1;
    resize: none;
    overflow: hidden;
    padding: 2px 6px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    font-family: var(--font-heading);
    font-stretch: 105%;
    font-size: 1.35rem;
    font-weight: 650;
    line-height: 1.3;
    field-sizing: content;
  }

  .title:hover {
    border-color: var(--border);
  }

  .title:focus {
    border-color: var(--accent);
    outline: none;
  }

  .fields {
    display: grid;
    grid-template-columns: 110px 1fr;
    align-items: center;
    gap: 8px 12px;
    margin: 0 0 18px;
  }

  dt {
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  dd {
    margin: 0;
    min-width: 0;
  }

  .value {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding: 3px 8px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
  }

  .value:hover {
    border-color: var(--border);
  }

  .date {
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }

  .date.overdue {
    color: var(--bad);
    font-weight: 600;
  }

  .dot {
    width: 9px;
    height: 9px;
    border-radius: 3px;
    flex: none;
  }

  section {
    margin-bottom: 20px;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 0.85rem;
    font-weight: 650;
  }

  .count {
    margin-left: 4px;
    font-size: 0.75rem;
    font-weight: 400;
    color: var(--text-muted);
  }

  .desc {
    resize: vertical;
    line-height: 1.5;
  }

  .desc-view {
    display: block;
    width: 100%;
    min-height: 40px;
    padding: 8px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    cursor: text;
  }

  .desc-view:hover {
    border-color: var(--border);
  }

  .subtasks {
    list-style: none;
    margin: 0 0 6px;
    padding: 0;
  }

  .subtasks li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 2px;
    border-bottom: 1px solid var(--border);
  }

  .subtasks li.done .sub-title {
    color: var(--text-muted);
    text-decoration: line-through;
  }

  .sub-title {
    flex: 1;
    border: 0;
    background: none;
    padding: 2px 0;
    text-align: left;
  }

  .sub-title:hover {
    text-decoration: underline;
  }

  .del {
    width: 22px;
    height: 22px;
    font-size: 11px;
    opacity: 0;
  }

  .subtasks li:hover .del,
  .del:focus-visible {
    opacity: 1;
  }

  .feed {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
  }

  .comment {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }

  .bubble {
    flex: 1;
    min-width: 0;
    padding: 8px 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
  }

  .meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 4px;
    font-size: 0.8rem;
  }

  .link {
    border: 0;
    background: none;
    padding: 0;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .link:hover {
    color: var(--text);
    text-decoration: underline;
  }

  .link.danger:hover {
    color: var(--bad);
  }

  .event {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding-left: 9px;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .event strong {
    color: var(--text);
    font-weight: 600;
  }

  .tick {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--border);
  }

  .when {
    margin-left: auto;
    white-space: nowrap;
  }

  .composer {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 10px 16px 12px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }

  .composer textarea {
    flex: 1;
    resize: none;
  }

  @media (max-width: 760px) {
    .fields {
      grid-template-columns: 92px 1fr;
    }

    .body {
      padding: 12px 14px 20px;
    }
  }
</style>
