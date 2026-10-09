<script lang="ts">
  import { api, type NewTask, type Status, type Task, type TaskPatch } from '../api';
  import { solid } from '../colors';
  import { failed, go, toast, workspace } from '../state.svelte';
  import ColorSwatches from './ColorSwatches.svelte';
  import Popover from './Popover.svelte';
  import Section from './Section.svelte';

  let { projectId }: { projectId: number } = $props();

  const project = $derived(workspace.projects.find((p) => p.id === projectId));
  let statuses = $state<Status[]>([]);
  let tasks = $state<Task[]>([]);
  let loading = $state(true);
  let colorOpen = $state(false);
  let menuOpen = $state(false);
  let renaming = $state(false);

  $effect(() => {
    Promise.all([api.statuses(projectId), api.tasks({ project: projectId })])
      .then(([s, t]) => {
        statuses = s;
        tasks = t;
      })
      .catch(failed)
      .finally(() => (loading = false));
  });

  const byStatus = $derived.by(() => {
    const groups = new Map<number, Task[]>(statuses.map((s) => [s.id, []]));
    for (const t of tasks) groups.get(t.status_id)?.push(t);
    for (const list of groups.values()) list.sort((a, b) => (a.position < b.position ? -1 : a.position > b.position ? 1 : a.id - b.id));
    return groups;
  });

  // ---- tasks (optimistic: apply locally, roll back if the server says no) ------------------

  async function updateTask(task: Task, patch: TaskPatch) {
    const before = $state.snapshot(task);
    const { completed, ...local }: TaskPatch & Partial<Task> = { ...patch };
    // A status change lands at the end of the new column; '~' sorts after every real key
    // until the server answers with the actual one.
    if (patch.status_id !== undefined && patch.status_id !== task.status_id) {
      local.position = '~';
      local.completed_at = categoryOf(patch.status_id) === 'done' ? new Date().toISOString() : null;
    }
    // The checkbox: guess the done column; reopening is left to the server's answer.
    if (completed !== undefined) {
      local.completed_at = completed ? new Date().toISOString() : null;
      const done = statuses.find((s) => s.category === 'done');
      if (completed && done) Object.assign(local, { status_id: done.id, position: '~' });
    }
    Object.assign(task, local);
    try {
      Object.assign(task, await api.updateTask(task.id, patch));
    } catch (e) {
      Object.assign(task, before);
      failed(e);
    }
  }

  async function createTask(t: NewTask): Promise<boolean> {
    try {
      tasks.push(await api.createTask({ project_id: projectId, ...t }));
      return true;
    } catch (e) {
      return failed(e);
    }
  }

  async function deleteTask(task: Task) {
    const i = tasks.indexOf(task);
    tasks.splice(i, 1);
    try {
      await api.deleteTask(task.id);
      toast(`Deleted “${task.title}”`);
    } catch (e) {
      tasks.splice(i, 0, task);
      failed(e);
    }
  }

  const categoryOf = (statusId: number) => statuses.find((s) => s.id === statusId)?.category;

  // ---- statuses ------------------------------------------------------------------------------

  async function updateStatus(status: Status, patch: Partial<Pick<Status, 'name' | 'color' | 'category'>>) {
    const before = $state.snapshot(status);
    Object.assign(status, patch);
    try {
      Object.assign(status, await api.updateStatus(status.id, patch));
      if (patch.category) tasks = await api.tasks({ project: projectId }); // completion changed
    } catch (e) {
      Object.assign(status, before);
      failed(e);
    }
  }

  async function addStatus(after: Status) {
    try {
      const s = await api.createStatus(projectId, { name: 'New section', color: 'slate', after_id: after.id });
      statuses.splice(statuses.indexOf(after) + 1, 0, s);
    } catch (e) {
      failed(e);
    }
  }

  async function deleteStatus(status: Status, moveTo?: number) {
    try {
      await api.deleteStatus(status.id, moveTo);
      statuses.splice(statuses.indexOf(status), 1);
      if (moveTo !== undefined) tasks = await api.tasks({ project: projectId });
    } catch (e) {
      failed(e);
    }
  }

  // ---- project -------------------------------------------------------------------------------

  async function updateProject(patch: { name?: string; color?: string; archived?: boolean }) {
    if (!project) return;
    const before = $state.snapshot(project);
    Object.assign(project, patch);
    try {
      Object.assign(project, await api.updateProject(project.id, patch));
    } catch (e) {
      Object.assign(project, before);
      failed(e);
    }
  }

  async function archive() {
    if (!project) return;
    menuOpen = false;
    try {
      await api.updateProject(project.id, { archived: true });
      workspace.projects.splice(workspace.projects.indexOf(project), 1);
      toast(`Archived “${project.name}”`);
      go({ name: 'home' });
    } catch (e) {
      failed(e);
    }
  }

  async function remove() {
    if (!project) return;
    menuOpen = false;
    const n = tasks.length;
    if (!confirm(`Delete “${project.name}” and its ${n} task${n === 1 ? '' : 's'}? This can't be undone. (Archive keeps them.)`)) return;
    try {
      await api.deleteProject(project.id);
      workspace.projects.splice(workspace.projects.indexOf(project), 1);
      go({ name: 'home' });
    } catch (e) {
      failed(e);
    }
  }

  function rename(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value.trim();
    renaming = false;
    if (value && value !== project?.name) updateProject({ name: value });
  }
</script>

{#if project}
  <header style:--project={solid(project.color)}>
    <Popover bind:open={colorOpen}>
      {#snippet trigger()}
        <button class="swatch" title="Project color" aria-label="Project color" onclick={() => (colorOpen = !colorOpen)}></button>
      {/snippet}
      <ColorSwatches
        value={project.color}
        onpick={(c) => {
          colorOpen = false;
          updateProject({ color: c });
        }}
      />
    </Popover>

    {#if renaming}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input title-input"
        value={project.name}
        autofocus
        onblur={rename}
        onkeydown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
          if (e.key === 'Escape') renaming = false;
        }}
      />
    {:else}
      <h1><button class="title" title="Rename" onclick={() => (renaming = true)}>{project.name}</button></h1>
    {/if}

    <div class="views" role="tablist" aria-label="View">
      <button role="tab" aria-selected="true" class="on">List</button>
      <button role="tab" aria-selected="false" disabled title="Board view arrives in M2">Board</button>
    </div>

    <Popover bind:open={menuOpen} align="right">
      {#snippet trigger()}
        <button class="icon-btn" aria-label="Project menu" onclick={() => (menuOpen = !menuOpen)}>⋯</button>
      {/snippet}
      <button class="item" onclick={() => ((menuOpen = false), (renaming = true))}>Rename</button>
      <button class="item" onclick={archive}>Archive</button>
      <hr />
      <button class="item danger" onclick={remove}>Delete project…</button>
    </Popover>
  </header>

  <div class="list">
    {#if loading}
      <p class="muted">Loading…</p>
    {:else}
      {#each statuses as status (status.id)}
        <Section
          {status}
          {statuses}
          tasks={byStatus.get(status.id) ?? []}
          onupdatetask={updateTask}
          oncreatetask={createTask}
          ondeletetask={deleteTask}
          onupdatestatus={(patch) => updateStatus(status, patch)}
          onaddstatus={() => addStatus(status)}
          ondeletestatus={(moveTo) => deleteStatus(status, moveTo)}
        />
      {/each}
    {/if}
  </div>
{:else}
  <p class="muted" style="padding: 24px">This project doesn't exist or was archived.</p>
{/if}

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 24px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 3px 0 var(--project);
  }

  .swatch {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    border: 0;
    background: var(--project);
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
    min-width: 0;
  }

  .title {
    border: 0;
    background: none;
    padding: 2px 4px;
    border-radius: 4px;
    font-weight: 650;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title:hover {
    background: var(--hover);
  }

  .title-input {
    max-width: 320px;
    font-size: 1.1rem;
    font-weight: 650;
  }

  .views {
    display: inline-flex;
    margin-left: auto;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }

  .views button {
    border: 0;
    background: transparent;
    padding: 4px 12px;
    border-radius: 4px;
    color: var(--text-muted);
  }

  .views button.on {
    background: var(--surface-2);
    color: var(--text);
    font-weight: 600;
  }

  .views button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .list {
    padding: 16px 24px 96px;
    max-width: 1100px;
  }

  @media (max-width: 760px) {
    header {
      padding-left: 52px;
    }

    .list {
      padding: 12px 8px 96px;
    }
  }
</style>
