<script lang="ts">
  import { api } from '../api';
  import { solid } from '../colors';
  import { live } from '../live.svelte';
  import { active, ProjectStore } from '../project.svelte';
  import { failed, go, toast, workspace, type View } from '../state.svelte';
  import BoardView from './BoardView.svelte';
  import ColorSwatches from './ColorSwatches.svelte';
  import ListView from './ListView.svelte';
  import Popover from './Popover.svelte';

  let { projectId, view }: { projectId: number; view: View } = $props();

  const project = $derived(workspace.projects.find((p) => p.id === projectId));
  let colorOpen = $state(false);
  let menuOpen = $state(false);
  let renaming = $state(false);

  // The view is keyed by project, so one store per mount; register it for live events.
  let store = $state<ProjectStore | null>(null);
  $effect(() => {
    const s = new ProjectStore(projectId);
    store = s;
    active.store = s;
    s.load();
    return () => {
      if (active.store === s) active.store = null;
    };
  });

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
    const n = store?.tasks.length ?? 0;
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
          if (c) updateProject({ color: c });
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

    <span
      class="live"
      class:on={live.connected}
      title={live.connected ? 'Live: changes from the other browser appear instantly' : 'Reconnecting…'}
    ></span>

    <div class="views" role="tablist" aria-label="View">
      {#each [['list', 'List'], ['board', 'Board']] as [v, label] (v)}
        <button
          role="tab"
          aria-selected={view === v}
          class:on={view === v}
          onclick={() => go({ name: 'project', id: projectId, view: v as View })}>{label}</button
        >
      {/each}
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

  {#if !store || store.loading}
    <p class="muted loading">Loading…</p>
  {:else if view === 'board'}
    <BoardView {store} />
  {:else}
    <ListView {store} />
  {/if}
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
    background: color-mix(in oklch, var(--surface) 88%, transparent);
    backdrop-filter: blur(8px);
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

  .live {
    margin-left: auto;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
    opacity: 0.5;
  }

  .live.on {
    background: var(--ok);
    opacity: 1;
  }

  .views {
    display: inline-flex;
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

  .loading {
    padding: 16px 24px;
    color: var(--on-canvas-muted);
  }

  @media (max-width: 760px) {
    header {
      padding-left: 52px;
    }
  }
</style>
