// Live sync: one EventSource per tab, applying the other browser's changes as they happen.
// The browser reconnects by itself after a dropped connection or a server restart; since
// events may have been missed meanwhile, every reconnect (and any `resync`) refetches.

import type { Project } from './api';
import { active } from './project.svelte';
import { loadWorkspace, workspace } from './state.svelte';

export const live = $state<{ connected: boolean }>({ connected: false });

let source: EventSource | null = null;

const KINDS = [
  'task.created',
  'task.updated',
  'task.moved',
  'task.deleted',
  'status.created',
  'status.updated',
  'status.deleted',
  'project.tasks_changed',
];

export function connect() {
  if (source) return;
  source = new EventSource('/api/events');
  let first = true;

  source.addEventListener('hello', () => {
    live.connected = true;
    if (!first) resync();
    first = false;
  });
  source.addEventListener('resync', resync);
  source.onerror = () => {
    live.connected = false;
  };

  for (const kind of KINDS) {
    source.addEventListener(kind, (e) => {
      const { data } = JSON.parse((e as MessageEvent).data);
      active.store?.apply(kind, data);
    });
  }

  source.addEventListener('project.created', (e) => {
    const p: Project = JSON.parse((e as MessageEvent).data).data;
    if (!workspace.projects.some((x) => x.id === p.id)) workspace.projects.push(p);
  });
  source.addEventListener('project.updated', (e) => {
    const p: Project = JSON.parse((e as MessageEvent).data).data;
    const i = workspace.projects.findIndex((x) => x.id === p.id);
    if (p.archived) {
      if (i >= 0) workspace.projects.splice(i, 1);
    } else if (i >= 0) {
      Object.assign(workspace.projects[i], p);
      workspace.projects.sort((a, b) => (a.position < b.position ? -1 : a.position > b.position ? 1 : a.id - b.id));
    } else {
      // Restored from the archive in the other browser.
      loadWorkspace();
    }
  });
  source.addEventListener('project.deleted', (e) => {
    const { id } = JSON.parse((e as MessageEvent).data).data;
    workspace.projects = workspace.projects.filter((p) => p.id !== id);
  });
}

export function disconnect() {
  source?.close();
  source = null;
  live.connected = false;
}

function resync() {
  loadWorkspace().catch(() => {});
  active.store?.load();
}
