<!-- Press Q anywhere: one line, parsed live (`Fix login !high @kat fri`), Enter to create. -->
<script lang="ts">
  import { autofocus } from '../actions';
  import { untrack } from 'svelte';
  import { api, type Task } from '../api';
  import { ink, PRIORITY_COLOR, solid, tint } from '../colors';
  import { parseQuickAdd } from '../quickadd';
  import { failed, labelIdsFor, people, toast, workspace } from '../state.svelte';

  let {
    projectId,
    onclose,
    oncreated,
  }: { projectId: number | undefined; onclose: () => void; oncreated: (task: Task) => void } = $props();

  let text = $state('');
  // Starts on the open project; the select can change it.
  let target = $state(untrack(() => projectId) ?? workspace.projects[0]?.id);
  let busy = $state(false);
  let dialog: HTMLDialogElement;

  const parsed = $derived(parseQuickAdd(text, people.users));

  $effect(() => {
    dialog.showModal();
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!parsed.title || target === undefined || busy) return;
    busy = true;
    try {
      const task = await api.createTask({
        project_id: target,
        title: parsed.title,
        priority: parsed.priority,
        assignee_id: parsed.assignee?.id,
        due_date: parsed.due_date,
        label_ids: await labelIdsFor(parsed.labels),
      });
      toast(`Added “${task.title}”`);
      oncreated(task);
      onclose();
    } catch (err) {
      failed(err);
    } finally {
      busy = false;
    }
  }

  function due(d: string) {
    return new Date(`${d}T00:00`).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
  }
</script>

<dialog bind:this={dialog} onclose={onclose} onclick={(e) => e.target === dialog && dialog.close()}>
  <form onsubmit={submit}>
    <input class="line" bind:value={text} placeholder="Task name  !high  @kat  #label  fri" aria-label="New task" use:autofocus />

    <div class="meta">
      <select class="project" bind:value={target} aria-label="Project" style:--dot={solid(workspace.projects.find((p) => p.id === target)?.color ?? 'slate')}>
        {#each workspace.projects as p (p.id)}
          <option value={p.id}>{p.name}</option>
        {/each}
      </select>
      {#if parsed.priority}
        {@const c = PRIORITY_COLOR[parsed.priority] ?? 'slate'}
        <span class="chip" style:background={tint(c, 20)} style:color={ink(c)}>!{parsed.priority}</span>
      {/if}
      {#if parsed.assignee}
        <span class="chip">@{parsed.assignee.display_name}</span>
      {/if}
      {#each parsed.labels as l (l)}
        <span class="chip">#{l}</span>
      {/each}
      {#if parsed.due_date}
        <span class="chip">📅 {due(parsed.due_date)}</span>
      {/if}
      <button class="btn primary" type="submit" disabled={!parsed.title || busy || target === undefined}>Add</button>
    </div>
    {#if workspace.projects.length === 0}
      <p class="muted">Create a project first.</p>
    {/if}
  </form>
</dialog>

<style>
  dialog {
    width: min(560px, calc(100vw - 32px));
    margin-top: 15vh;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 0.3);
  }

  form {
    padding: 14px;
    display: grid;
    gap: 12px;
  }

  .line {
    width: 100%;
    border: 0;
    outline: none;
    background: transparent;
    font-size: 1.1rem;
    padding: 4px 2px;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .project {
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    border-left: 4px solid var(--dot);
  }

  .chip {
    padding: 3px 8px;
    border-radius: 999px;
    font-size: 0.8rem;
    background: var(--surface-2);
  }

  .btn {
    margin-left: auto;
  }

  p {
    margin: 0;
  }
</style>
