<script lang="ts">
  import { failed, go, loadWorkspace, router, workspace } from '../state.svelte';
  import ProjectView from './ProjectView.svelte';
  import QuickAdd from './QuickAdd.svelte';
  import Sidebar from './Sidebar.svelte';

  let loaded = $state(false);
  let navOpen = $state(false);
  let quickAdd = $state(false);
  /** Bumped when quick add creates a task, so the open project reloads. */
  let refresh = $state(0);

  loadWorkspace()
    .catch(failed)
    .finally(() => (loaded = true));

  // With no project in the URL, open the first one.
  $effect(() => {
    if (loaded && router.route.name === 'home' && workspace.projects.length > 0) {
      go({ name: 'project', id: workspace.projects[0].id });
    }
  });

  // Close the mobile nav after navigating.
  $effect(() => {
    void router.route;
    navOpen = false;
  });

  function onKey(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    const typing = el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName);
    if (typing || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === 'q' || e.key === 'Q') {
      e.preventDefault();
      quickAdd = true;
    }
  }

  const currentProjectId = $derived(router.route.name === 'project' ? router.route.id : undefined);
</script>

<svelte:window onkeydown={onKey} />

<div class="shell" class:nav-open={navOpen}>
  <Sidebar onquickadd={() => (quickAdd = true)} />
  <button class="scrim" aria-label="Close menu" onclick={() => (navOpen = false)}></button>

  <main>
    <button class="icon-btn menu" aria-label="Open menu" onclick={() => (navOpen = true)}>☰</button>
    {#if !loaded}
      <p class="muted pad">Loading…</p>
    {:else if currentProjectId !== undefined}
      {#key `${currentProjectId}:${refresh}`}
        <ProjectView projectId={currentProjectId} />
      {/key}
    {:else}
      <div class="empty">
        <h2>No projects yet</h2>
        <p class="muted">Create one in the sidebar to get started.</p>
      </div>
    {/if}
  </main>
</div>

{#if quickAdd}
  <QuickAdd
    projectId={currentProjectId}
    onclose={() => (quickAdd = false)}
    oncreated={(task) => {
      if (task.project_id === currentProjectId) refresh++;
      else go({ name: 'project', id: task.project_id });
    }}
  />
{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar) 1fr;
    min-height: 100vh;
  }

  main {
    min-width: 0;
    position: relative;
  }

  .menu,
  .scrim {
    display: none;
  }

  .pad {
    padding: 24px;
  }

  .empty {
    padding: 64px 24px;
    text-align: center;
  }

  @media (max-width: 760px) {
    .shell {
      grid-template-columns: 1fr;
    }

    .shell > :global(nav) {
      position: fixed;
      inset: 0 auto 0 0;
      width: min(var(--sidebar), 85vw);
      z-index: 40;
      transform: translateX(-100%);
      transition: transform 0.18s ease;
    }

    .shell.nav-open > :global(nav) {
      transform: none;
    }

    .shell.nav-open .scrim {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 35;
      border: 0;
      background: rgb(0 0 0 / 0.35);
    }

    /* Fixed above the sticky project header so it's always reachable. */
    .menu {
      display: inline-grid;
      position: fixed;
      top: 12px;
      left: 12px;
      z-index: 20;
    }
  }
</style>
