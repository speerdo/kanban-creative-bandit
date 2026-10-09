<script lang="ts">
  import { connect, disconnect } from '../live.svelte';
  import { active } from '../project.svelte';
  import { defaultView, failed, go, loadWorkspace, router, workspace } from '../state.svelte';
  import { handleShortcut } from '../shortcuts';
  import CalendarView from './CalendarView.svelte';
  import ListPage from './ListPage.svelte';
  import MyTasks from './MyTasks.svelte';
  import ProjectView from './ProjectView.svelte';
  import Settings from './Settings.svelte';
  import ShortcutSheet from './ShortcutSheet.svelte';
  import TaskDetail from './TaskDetail.svelte';
  import QuickAdd from './QuickAdd.svelte';
  import Sidebar from './Sidebar.svelte';

  let loaded = $state(false);
  let navOpen = $state(false);
  let quickAdd = $state(false);
  let help = $state(false);

  $effect(() => {
    connect();
    return disconnect;
  });

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
    if (quickAdd || help) return; // their dialogs own the keyboard
    const handled = handleShortcut(
      e,
      () => (quickAdd = true),
      () => (help = true),
    );
    if (handled) e.preventDefault();
  }

  const current = $derived(router.route.name === 'project' ? router.route : undefined);
  const openTaskId = $derived(
    (router.route.name === 'project' || router.route.name === 'my' || router.route.name === 'calendar') && router.route.taskId
      ? router.route.taskId
      : null,
  );
  const currentProjectId = $derived(current?.id);

  // `#/lists` opens the last list used here, or the first one.
  const lastList = () => {
    try {
      return Number(localStorage.getItem('lists.last')) || 0;
    } catch {
      return 0;
    }
  };
  const openList = $derived(
    router.route.name === 'list'
      ? router.route.id
        ? workspace.lists.find((l) => l.id === (router.route as { id: number }).id)
        : (workspace.lists.find((l) => l.id === lastList()) ?? workspace.lists[0])
      : undefined,
  );
</script>

<svelte:window onkeydown={onKey} />

<div class="shell" class:nav-open={navOpen}>
  <Sidebar onquickadd={() => (quickAdd = true)} />
  <button class="scrim" aria-label="Close menu" onclick={() => (navOpen = false)}></button>

  <main>
    <button class="icon-btn menu" aria-label="Open menu" onclick={() => (navOpen = true)}>☰</button>
    {#if !loaded}
      <p class="muted pad">Loading…</p>
    {:else if router.route.name === 'settings'}
      <Settings />
    {:else if router.route.name === 'my'}
      <MyTasks />
    {:else if router.route.name === 'calendar'}
      <CalendarView />
    {:else if router.route.name === 'list'}
      {#if openList}
        {#key openList.id}
          <ListPage list={openList} />
        {/key}
      {:else if workspace.lists.length === 0}
        <div class="empty">
          <h2>No lists yet</h2>
          <p class="muted">Make one in the sidebar, for example Groceries.</p>
        </div>
      {:else}
        <p class="muted pad">That list was deleted.</p>
      {/if}
    {:else if current}
      {#key current.id}
        <ProjectView projectId={current.id} view={current.view ?? defaultView()} />
      {/key}
    {:else}
      <div class="empty">
        <h2>No projects yet</h2>
        <p class="muted">Create one in the sidebar to get started.</p>
      </div>
    {/if}
  </main>
</div>

{#if openTaskId !== null}
  {#key openTaskId}
    <TaskDetail taskId={openTaskId} />
  {/key}
{/if}

{#if help}
  <ShortcutSheet onclose={() => (help = false)} />
{/if}

{#if quickAdd}
  <QuickAdd
    projectId={currentProjectId}
    onclose={() => (quickAdd = false)}
    oncreated={(task) => {
      // Shown right away; the live echo of the same task is a no-op.
      if (task.project_id === currentProjectId) active.store?.apply('task.created', task);
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
    color: var(--on-canvas-muted);
  }

  .empty {
    padding: 64px 24px;
    text-align: center;
    color: var(--on-canvas);
  }

  .empty .muted {
    color: var(--on-canvas-muted);
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
      background: var(--surface);
      position: fixed;
      top: 12px;
      left: 12px;
      z-index: 20;
    }
  }
</style>
