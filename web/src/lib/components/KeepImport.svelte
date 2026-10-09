<!-- Settings → Import from Google Keep: pick the Takeout .zip, preview, then import. Checklists
     become Lists (merged into a list of the same name); other notes become tasks. -->
<script lang="ts">
  import { api, type KeepNote, type KeepReport } from '../api';
  import { readKeep } from '../keep';
  import { failed, loadWorkspace, toast, workspace } from '../state.svelte';

  let notes = $state<KeepNote[] | null>(null);
  let preview = $state<KeepReport | null>(null);
  let project = $state<number | null>(null);
  let includeArchived = $state(false);
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (project === null && workspace.projects.length) project = workspace.projects[0].id;
  });

  async function pick() {
    const files = [...(input?.files ?? [])];
    if (!files.length) return;
    busy = true;
    try {
      notes = await readKeep(files);
      if (!notes.length) {
        toast("No Keep notes found in that file. Pick the Takeout .zip, or the .json files in its Keep folder.", 'error');
        notes = null;
        return;
      }
      await dryRun();
    } catch (e) {
      failed(e);
    } finally {
      busy = false;
    }
  }

  async function dryRun() {
    if (!notes) return;
    preview = await api
      .importKeep({ notes, project_id: project, include_archived: includeArchived, dry_run: true })
      .catch((e) => (failed(e), null));
  }

  async function run() {
    if (!notes) return;
    busy = true;
    try {
      const r = await api.importKeep({ notes, project_id: project, include_archived: includeArchived, dry_run: false });
      toast(`Imported ${r.lists.length} list${r.lists.length === 1 ? '' : 's'} and ${r.tasks.length} task${r.tasks.length === 1 ? '' : 's'}`);
      notes = null;
      preview = null;
      if (input) input.value = '';
      await loadWorkspace();
    } catch (e) {
      failed(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="panel">
  <h2>Import from Google Keep</h2>
  <p class="hint">
    Optional, one-off. At <a href="https://takeout.google.com/" target="_blank" rel="noopener noreferrer">takeout.google.com</a>
    click <em>Deselect all</em>, tick <strong>Keep</strong>, export, and pick the .zip here. Checklists become Lists (added
    to a list with the same name if there is one); other notes become tasks.
  </p>
  <input bind:this={input} type="file" accept=".zip,.json,application/zip,application/json" multiple onchange={pick} disabled={busy} />

  {#if notes && preview}
    <div class="keep-preview">
      <div class="row">
        <label>
          Other notes become tasks in
          <select class="input" bind:value={project} onchange={dryRun}>
            <option value={null}>Nowhere (skip them)</option>
            {#each workspace.projects as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </select>
        </label>
        <label class="check"><input type="checkbox" bind:checked={includeArchived} onchange={dryRun} /> Include archived notes</label>
      </div>

      <p><strong>{notes.length}</strong> notes found. This will:</p>
      <ul>
        {#each preview.lists as l (l.name)}
          <li>{l.merged ? 'add' : 'create the list'} <strong>{l.name}</strong>{l.merged ? ' ← ' : ' with '}{l.items} item{l.items === 1 ? '' : 's'}</li>
        {/each}
        {#if preview.tasks.length}
          <li>
            create {preview.tasks.length} task{preview.tasks.length === 1 ? '' : 's'}:
            <span class="muted">{preview.tasks.slice(0, 5).join(' · ')}{preview.tasks.length > 5 ? ' …' : ''}</span>
          </li>
        {/if}
        {#if preview.skipped}<li class="muted">skip {preview.skipped} (archived, trashed or empty)</li>{/if}
      </ul>
      <div class="actions">
        <button class="btn primary" disabled={busy || (!preview.lists.length && !preview.tasks.length)} onclick={run}>
          {busy ? 'Importing…' : 'Import'}
        </button>
        <button class="btn" onclick={() => ((notes = null), (preview = null), input && (input.value = ''))}>Cancel</button>
      </div>
    </div>
  {/if}
</section>

<style>
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

  .hint {
    margin: 0 0 12px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .hint a {
    color: inherit;
  }

  .keep-preview {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    font-size: 0.9rem;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    align-items: center;
  }

  .row label {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  select {
    width: auto;
  }

  ul {
    margin: 6px 0 12px;
    padding-left: 20px;
  }

  .muted {
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    gap: 8px;
  }
</style>
