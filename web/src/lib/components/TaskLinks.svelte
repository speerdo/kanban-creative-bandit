<!-- Links on a task: Gmail threads, Google Docs/Sheets/Slides/Forms, Drive, or any web address.
     Paste a URL to add one; the kind comes from the address. -->
<script lang="ts">
  import { autofocus } from '../actions';
  import { api, type LinkKind, type TaskLink } from '../api';
  import { listen } from '../live.svelte';
  import { failed } from '../state.svelte';

  let { taskId }: { taskId: number } = $props();

  let links = $state<TaskLink[]>([]);
  let adding = $state(false);
  let url = $state('');
  let busy = $state(false);

  async function load() {
    links = await api.links(taskId).catch((e) => (failed(e), links));
  }

  $effect(() => {
    void taskId;
    load();
    return listen({
      event(kind, data) {
        if (data.task_id !== taskId) return;
        if (kind === 'link.created' && !links.some((l) => l.id === data.id)) links.push(data as unknown as TaskLink);
        if (kind === 'link.deleted') links = links.filter((l) => l.id !== data.id);
      },
      resync: load,
    });
  });

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const u = url.trim();
    if (!u) return;
    busy = true;
    try {
      const link = await api.addLink(taskId, /^https?:\/\//i.test(u) ? u : `https://${u}`);
      if (!links.some((l) => l.id === link.id)) links.push(link);
      url = '';
      adding = false;
    } catch (err) {
      failed(err);
    } finally {
      busy = false;
    }
  }

  async function remove(link: TaskLink) {
    links = links.filter((l) => l.id !== link.id);
    await api.deleteLink(taskId, link.id).catch((e) => {
      links.push(link);
      failed(e);
    });
  }

  const KINDS: Record<LinkKind, { icon: string; name: string; color: string }> = {
    gmail: { icon: '✉', name: 'Email', color: '#ea4335' },
    doc: { icon: '≡', name: 'Google Doc', color: '#4285f4' },
    sheet: { icon: '▦', name: 'Google Sheet', color: '#0f9d58' },
    slides: { icon: '▭', name: 'Google Slides', color: '#f4b400' },
    form: { icon: '☑', name: 'Google Form', color: '#7248b9' },
    drive: { icon: '▲', name: 'Drive', color: '#1fa463' },
    url: { icon: '↗', name: 'Link', color: 'var(--text-muted)' },
  };

  function label(l: TaskLink): string {
    if (l.title) return l.title;
    if (l.kind === 'url') {
      try {
        const u = new URL(l.url);
        return u.hostname.replace(/^www\./, '') + (u.pathname.length > 1 ? u.pathname : '');
      } catch {
        return l.url;
      }
    }
    return KINDS[l.kind].name;
  }
</script>

<section>
  <h3>Links</h3>
  {#if links.length}
    <ul class="links">
      {#each links as l (l.id)}
        <li>
          <a href={l.url} target="_blank" rel="noopener noreferrer" title={l.url}>
            <span class="icon" style:color={KINDS[l.kind].color} aria-hidden="true">{KINDS[l.kind].icon}</span>
            <span class="title">{label(l)}</span>
            {#if l.title && l.kind !== 'url'}<span class="kind">{KINDS[l.kind].name}</span>{/if}
          </a>
          <button class="icon-btn" aria-label="Remove link {label(l)}" title="Remove" onclick={() => remove(l)}>✕</button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if adding}
    <form onsubmit={add}>
      <input
        class="input"
        type="text"
        inputmode="url"
        aria-label="Link address"
        placeholder="Paste a Google Doc, Drive, Gmail or any link"
        bind:value={url}
        use:autofocus
        disabled={busy}
        onkeydown={(e) => e.key === 'Escape' && ((adding = false), e.stopPropagation())}
      />
    </form>
  {:else}
    <button class="add" onclick={() => (adding = true)}>＋ Add link</button>
  {/if}
</section>

<style>
  section {
    margin-bottom: 20px;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 0.85rem;
    font-weight: 650;
  }

  .links {
    list-style: none;
    margin: 0 0 6px;
    padding: 0;
    display: grid;
    gap: 4px;
  }

  li {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  a {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
    text-decoration: none;
    background: var(--surface);
  }

  a:hover {
    background: var(--hover);
  }

  .icon {
    flex: none;
    width: 18px;
    text-align: center;
    font-weight: 700;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .kind {
    margin-left: auto;
    flex: none;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .add {
    border: 0;
    background: transparent;
    padding: 4px 0;
    color: var(--text-muted);
  }

  .add:hover {
    color: var(--text);
  }

  form .input {
    width: 100%;
  }
</style>
