<script lang="ts">
  import { autofocus } from '../actions';
  import { api } from '../api';
  import { solid } from '../colors';
  import { defaultView, failed, go, router, session, workspace } from '../state.svelte';
  import Avatar from './Avatar.svelte';

  let { onquickadd }: { onquickadd: () => void } = $props();

  let adding = $state(false);
  let name = $state('');

  async function create(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    try {
      // Rotate through the palette so new projects don't all look the same.
      const colors = ['blue', 'green', 'violet', 'orange', 'teal', 'pink', 'amber', 'cyan', 'red', 'lime'];
      const p = await api.createProject({ name, color: colors[workspace.projects.length % colors.length] });
      workspace.projects.push(p);
      name = '';
      adding = false;
      go({ name: 'project', id: p.id });
    } catch (err) {
      failed(err);
    }
  }

  async function logout() {
    await api.logout().catch(() => {});
    session.me = null;
  }

  const current = $derived(router.route.name === 'project' ? router.route.id : null);
  // Switching projects keeps the current view.
  const view = $derived((router.route.name === 'project' && router.route.view) || defaultView());
</script>

<nav>
  <div class="brand">
    <img src="/favicon.svg" alt="" width="22" height="22" />
    <span>Kanban</span>
    <button class="icon-btn add" title="Quick add (Q)" aria-label="Quick add task" onclick={onquickadd}>＋</button>
  </div>

  <a href="#/my" class="my" class:active={router.route.name === 'my'} aria-current={router.route.name === 'my' ? 'page' : undefined}>
    <span class="my-icon" aria-hidden="true">✓</span>My tasks
  </a>

  <h3>Projects</h3>
  <ul>
    {#each workspace.projects as p (p.id)}
      <li>
        <a href="#/p/{p.id}/{view}" class:active={p.id === current} aria-current={p.id === current ? 'page' : undefined}>
          <span class="dot" style:background={solid(p.color)}></span>
          <span class="name">{p.name}</span>
        </a>
      </li>
    {/each}
  </ul>

  {#if adding}
    <form onsubmit={create}>
      <input
        class="input"
        placeholder="Project name"
        bind:value={name}
        use:autofocus
        onkeydown={(e) => e.key === 'Escape' && (adding = false)}
        onblur={() => !name.trim() && (adding = false)}
      />
    </form>
  {:else}
    <button class="new" onclick={() => (adding = true)}>＋ New project</button>
  {/if}

  <div class="me">
    <Avatar user={session.me ?? undefined} size={26} />
    <span class="name">{session.me?.display_name}</span>
    <a
      class="icon-btn settings"
      href="#/settings"
      title="Settings"
      aria-label="Settings"
      aria-current={router.route.name === 'settings' ? 'page' : undefined}>⚙</a
    >
    <button class="icon-btn" title="Sign out" aria-label="Sign out" onclick={logout}>⎋</button>
  </div>
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 10px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    height: 100vh;
    position: sticky;
    top: 0;
    overflow-y: auto;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 6px 12px;
    font-family: var(--font-heading);
    font-stretch: 125%;
    font-weight: 800;
    font-size: 1.05rem;
    text-transform: uppercase;
    letter-spacing: 0.01em;
  }

  .add {
    margin-left: auto;
    font-size: 1.1rem;
  }

  h3 {
    margin: 8px 8px 4px;
    font-family: var(--font-mono);
    font-size: 0.7rem;
    font-weight: 400;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  a,
  .new {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    text-decoration: none;
    text-align: left;
  }

  a:hover,
  .new:hover {
    background: var(--hover);
  }

  a.active {
    background: var(--surface-2);
    font-weight: 600;
  }

  .new {
    color: var(--text-muted);
  }

  .my-icon {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1.5px solid currentColor;
    font-size: 9px;
    font-weight: 700;
  }

  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  form {
    padding: 2px 4px;
  }

  .me {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: auto;
    padding: 10px 6px 0;
    border-top: 1px solid var(--border);
  }

  .me .name {
    flex: 1;
  }

  .me .settings {
    width: 28px;
    padding: 0;
    justify-content: center;
    font-size: 1rem;
  }

  .me .settings[aria-current='page'] {
    background: var(--surface-2);
    color: var(--text);
  }
</style>
