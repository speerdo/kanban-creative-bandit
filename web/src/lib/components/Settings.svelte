<!-- Settings: appearance (applied live as you click), default view, labels, Google, and profile. Prefs are
     saved per user on the server, so they follow you to every device. -->
<script lang="ts">
  import { api, type Label, type Prefs, type Status, type Task } from '../api';
  import { COLORS, solid } from '../colors';
  import { failed, session, workspace } from '../state.svelte';
  import Popover from './Popover.svelte';
  import Avatar from './Avatar.svelte';
  import ColorSwatches from './ColorSwatches.svelte';
  import GoogleSettings from './GoogleSettings.svelte';
  import KeepImport from './KeepImport.svelte';
  import StatusPill from './StatusPill.svelte';

  const prefs = $derived(session.me!.prefs);

  // Changes apply at once (App re-themes on any prefs change); saving is debounced so a run
  // of clicks becomes one request.
  let pending: Partial<Prefs> = {};
  let timer: ReturnType<typeof setTimeout> | undefined;
  let saved = $state<'idle' | 'saving' | 'saved'>('idle');

  function set(patch: Partial<Prefs>) {
    Object.assign(session.me!.prefs, patch);
    Object.assign(pending, patch);
    saved = 'saving';
    clearTimeout(timer);
    timer = setTimeout(save, 400);
  }

  async function save() {
    const patch = pending;
    pending = {};
    try {
      const p = await api.updatePrefs(patch);
      Object.assign(session.me!.prefs, p);
      saved = 'saved';
    } catch (e) {
      failed(e);
      // Put back what the server has.
      Object.assign(session.me!, await api.me().catch(() => session.me!));
      saved = 'idle';
    }
  }

  let name = $state(session.me!.display_name);

  async function saveProfile(patch: { display_name?: string; avatar_color?: string }) {
    try {
      const me = await api.updateProfile(patch);
      Object.assign(session.me!, me);
      name = me.display_name;
    } catch (e) {
      failed(e);
      name = session.me!.display_name;
    }
  }

  // ---- labels (shared by both of us) ----------------------------------------------------------

  let labelColorOpen = $state<number | null>(null);

  async function updateLabel(l: Label, patch: { name?: string; color?: string }) {
    const before = { ...l };
    Object.assign(l, patch);
    try {
      Object.assign(l, await api.updateLabel(l.id, patch));
    } catch (e) {
      Object.assign(l, before);
      failed(e);
    }
  }

  async function deleteLabel(l: Label) {
    if (!confirm(`Delete the label “${l.name}”? It comes off every task that has it.`)) return;
    try {
      await api.deleteLabel(l.id);
      workspace.labels = workspace.labels.filter((x) => x.id !== l.id);
    } catch (e) {
      failed(e);
    }
  }

  // Sample data for the preview.
  const sampleStatuses: Status[] = [
    { id: -1, project_id: 0, name: 'In Progress', color: 'amber', category: 'in_progress', position: 'V' },
  ];
  const sample: Pick<Task, 'title' | 'priority'> = { title: 'Pick up paint samples', priority: 'high' };

  type Choice<T> = [T, string];
  const THEMES: Choice<Prefs['theme']>[] = [
    ['system', 'System'],
    ['light', 'Light'],
    ['dark', 'Dark'],
  ];
  const STYLES: Choice<Prefs['background_style']>[] = [
    ['solid', 'Solid'],
    ['gradient', 'Gradient'],
    ['subtle-pattern', 'Pattern'],
  ];
  const DENSITIES: Choice<Prefs['density']>[] = [
    ['comfortable', 'Comfortable'],
    ['compact', 'Compact'],
  ];
  const VIEWS: Choice<Prefs['default_view']>[] = [
    ['list', 'List'],
    ['board', 'Board'],
  ];
</script>

{#snippet segmented<T extends string>(label: string, options: Choice<T>[], value: T, pick: (v: T) => void)}
  <div class="segmented" role="radiogroup" aria-label={label}>
    {#each options as [v, text] (v)}
      <button role="radio" aria-checked={value === v} class:on={value === v} onclick={() => pick(v)}>{text}</button>
    {/each}
  </div>
{/snippet}

<header class="page-head">
  <h1>Settings</h1>
  <span class="saved mono" aria-live="polite">
    {saved === 'saving' ? 'Saving…' : saved === 'saved' ? 'Saved' : ''}
  </span>
</header>

<div class="page">
  <div class="panels">
    <section class="panel">
      <h2>Appearance</h2>

      <div class="field">
        <span class="label">Theme</span>
        {@render segmented('Theme', THEMES, prefs.theme, (v) => set({ theme: v }))}
      </div>

      <div class="field">
        <span class="label">Accent</span>
        <ColorSwatches value={prefs.accent_color} onpick={(c) => c && set({ accent_color: c })} />
      </div>

      <div class="field">
        <span class="label">Background</span>
        <ColorSwatches value={prefs.background_color} none="Default" onpick={(c) => set({ background_color: c })} />
        {@render segmented('Background style', STYLES, prefs.background_style, (v) => set({ background_style: v }))}
        <p class="hint">Only the canvas behind the panels changes, so cards and text stay readable.</p>
      </div>

      <div class="field">
        <span class="label">Density</span>
        {@render segmented('Density', DENSITIES, prefs.density, (v) => set({ density: v }))}
      </div>
    </section>

    <section class="panel">
      <h2>Workspace</h2>
      <div class="field">
        <span class="label">Projects open in</span>
        {@render segmented('Default view', VIEWS, prefs.default_view, (v) => set({ default_view: v }))}
      </div>
    </section>

    <section class="panel">
      <h2>Labels</h2>
      {#if workspace.labels.length === 0}
        <p class="hint">No labels yet. Add them from a task, or type <code>#name</code> in quick add.</p>
      {:else}
        <ul class="labels">
          {#each workspace.labels as l (l.id)}
            <li>
              <Popover open={labelColorOpen === l.id}>
                {#snippet trigger()}
                  <button
                    class="label-swatch"
                    style:background={solid(l.color)}
                    aria-label="Color of {l.name}"
                    onclick={() => (labelColorOpen = labelColorOpen === l.id ? null : l.id)}
                  ></button>
                {/snippet}
                <ColorSwatches
                  value={l.color}
                  onpick={(c) => {
                    labelColorOpen = null;
                    if (c) updateLabel(l, { color: c });
                  }}
                />
              </Popover>
              <input
                class="input"
                value={l.name}
                aria-label="Label name"
                maxlength="40"
                onblur={(e) => {
                  const v = e.currentTarget.value.trim();
                  if (v && v !== l.name) updateLabel(l, { name: v });
                  else e.currentTarget.value = l.name;
                }}
                onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
              />
              <button class="icon-btn" aria-label="Delete label {l.name}" onclick={() => deleteLabel(l)}>✕</button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <GoogleSettings />

    <KeepImport />

    <section class="panel">
      <h2>Profile</h2>
      <label class="field">
        <span class="label">Display name</span>
        <input
          class="input"
          bind:value={name}
          maxlength="60"
          onblur={() => name.trim() && name.trim() !== session.me!.display_name && saveProfile({ display_name: name })}
          onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
        />
      </label>
      <div class="field">
        <span class="label">Avatar color</span>
        <ColorSwatches value={session.me!.avatar_color} onpick={(c) => c && saveProfile({ avatar_color: c })} />
      </div>
      <p class="hint">Change your password on the server: <code>kanban user passwd {session.me!.username}</code></p>
    </section>
  </div>

  <aside class="preview" aria-label="Preview">
    <span class="label">Preview</span>
    <div class="preview-canvas">
      <div class="card">
        <div class="row">
          <span class="check" aria-hidden="true"></span>
          <span class="title">{sample.title}</span>
          <Avatar user={session.me ?? undefined} size={22} />
        </div>
        <div class="row meta">
          <StatusPill status={sampleStatuses[0]} statuses={sampleStatuses} onchange={() => {}} />
          <span class="due">Today</span>
        </div>
      </div>
      <button class="btn primary">Accent button</button>
      <p class="on-canvas">Text on the canvas</p>
      <div class="dots" aria-hidden="true">
        {#each COLORS as c (c)}<span style:background="var(--c-{c})" style:color="var(--c-{c}-on)">A</span>{/each}
      </div>
    </div>
  </aside>
</div>

<style>
  .page-head {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 24px;
    background: color-mix(in oklch, var(--surface) 88%, transparent);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--border);
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
  }

  .saved {
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .page {
    display: grid;
    grid-template-columns: minmax(0, 560px) minmax(0, 320px);
    gap: 20px;
    padding: 20px 24px 64px;
    align-items: start;
  }

  .panels {
    display: grid;
    gap: 16px;
  }

  .panel {
    padding: 18px 20px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1rem;
  }

  .field {
    display: grid;
    gap: 8px;
    margin-bottom: 18px;
  }

  .field:last-child {
    margin-bottom: 0;
  }

  .label {
    font-family: var(--font-mono);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .hint {
    margin: 0;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .segmented {
    display: inline-flex;
    justify-self: start;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }

  .segmented button {
    border: 0;
    background: transparent;
    padding: 5px 14px;
    border-radius: 4px;
    color: var(--text-muted);
  }

  .segmented button.on {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 2px var(--shadow-2);
  }

  .labels {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }

  .labels li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .labels .input {
    padding: 5px 8px;
  }

  .label-swatch {
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 5px;
  }

  .preview {
    position: sticky;
    top: 76px;
    display: grid;
    gap: 8px;
  }

  .preview .label {
    color: var(--on-canvas-muted);
  }

  .preview-canvas {
    display: grid;
    gap: 14px;
    justify-items: start;
    padding: 18px;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
  }

  .card {
    width: 100%;
    padding: var(--card-pad) 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: inset 3px 0 0 var(--c-orange);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .meta {
    margin-top: 8px;
    padding-left: 26px;
  }

  .check {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 1.5px solid var(--text-muted);
  }

  .title {
    flex: 1;
  }

  .due {
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .on-canvas {
    margin: 0;
    color: var(--on-canvas);
  }

  .dots {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .dots span {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    font-size: 0.7rem;
    font-weight: 700;
  }

  @media (max-width: 900px) {
    .page {
      grid-template-columns: minmax(0, 1fr);
    }

    .preview {
      position: static;
      order: -1;
    }
  }

  @media (max-width: 760px) {
    .page-head {
      padding-left: 52px;
    }

    .page {
      padding: 12px 12px 48px;
    }
  }
</style>
