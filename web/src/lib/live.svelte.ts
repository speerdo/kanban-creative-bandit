// Live sync: one EventSource per tab, applying the other browser's changes as they happen.
// The browser reconnects by itself after a dropped connection or a server restart; since
// events may have been missed meanwhile, every reconnect (and any `resync`) refetches.

import type { Label, Prefs, Project, User } from './api';
import { loadWorkspace, people, session, workspace } from './state.svelte';

export const live = $state<{ connected: boolean }>({ connected: false });

let source: EventSource | null = null;

/** A view that shows tasks (a project, My Tasks, the detail panel). */
export type Listener = {
  /** A task/status/comment event; ignore what isn't yours. */
  event: (kind: string, data: Record<string, unknown>) => void;
  /** Events may have been missed: refetch. */
  resync: () => void;
};
const listeners = new Set<Listener>();

/** Subscribes a view; returns the unsubscribe function (handy as an $effect cleanup). */
export function listen(l: Listener): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

const KINDS = [
  'task.created',
  'task.updated',
  'task.moved',
  'task.deleted',
  'status.created',
  'status.updated',
  'status.deleted',
  'project.tasks_changed',
  'comment.created',
  'comment.updated',
  'comment.deleted',
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
      for (const l of listeners) l.event(kind, data);
    });
  }

  // My prefs changed on another device (or this one: the echo is harmless).
  source.addEventListener('prefs.updated', (e) => {
    const { by, data } = JSON.parse((e as MessageEvent).data) as { by: number; data: Prefs };
    if (session.me && by === session.me.id) Object.assign(session.me.prefs, data);
  });
  source.addEventListener('user.updated', (e) => {
    const user: User = JSON.parse((e as MessageEvent).data).data;
    const known = people.users.find((u) => u.id === user.id);
    if (known) Object.assign(known, user);
    if (session.me?.id === user.id) Object.assign(session.me, user);
  });

  const labelSort = () => workspace.labels.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
  source.addEventListener('label.created', (e) => {
    const l: Label = JSON.parse((e as MessageEvent).data).data;
    if (!workspace.labels.some((x) => x.id === l.id)) workspace.labels.push(l);
    labelSort();
  });
  source.addEventListener('label.updated', (e) => {
    const l: Label = JSON.parse((e as MessageEvent).data).data;
    const known = workspace.labels.find((x) => x.id === l.id);
    if (known) Object.assign(known, l);
    labelSort();
  });
  source.addEventListener('label.deleted', (e) => {
    const { id } = JSON.parse((e as MessageEvent).data).data;
    workspace.labels = workspace.labels.filter((l) => l.id !== id);
    for (const l of listeners) l.resync(); // tasks lose the label
  });

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
  for (const l of listeners) l.resync();
}
