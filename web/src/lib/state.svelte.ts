// App-wide state: who's signed in, the people and projects, toasts, and the current route.

import { api, ApiError, type Project, type User } from './api';

export const session = $state<{ me: User | null; checked: boolean }>({ me: null, checked: false });
export const people = $state<{ users: User[] }>({ users: [] });
export const workspace = $state<{ projects: Project[] }>({ projects: [] });

export function userById(id: number | null | undefined): User | undefined {
  return id == null ? undefined : people.users.find((u) => u.id === id);
}

export async function loadWorkspace() {
  const [users, projects] = await Promise.all([api.users(), api.projects()]);
  people.users = users;
  workspace.projects = projects;
}

// ---- toasts -------------------------------------------------------------------------------

export type Toast = { id: number; text: string; kind: 'info' | 'error' };
export const toasts = $state<Toast[]>([]);
let nextToast = 1;

export function toast(text: string, kind: Toast['kind'] = 'info') {
  const id = nextToast++;
  toasts.push({ id, text, kind });
  setTimeout(() => dismiss(id), kind === 'error' ? 6000 : 3000);
}

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

/** Toast an API failure; returns false so callers can `if (!(await attempt(...)))`. */
export function failed(e: unknown): false {
  if (!(e instanceof ApiError && e.status === 401)) {
    toast(e instanceof Error ? e.message : String(e), 'error');
  }
  return false;
}

// ---- routing (hash based: #/p/3 or #/p/3/board) -------------------------------------------

export type View = 'list' | 'board';
export type Route = { name: 'home' } | { name: 'project'; id: number; view: View };

const VIEW_KEY = 'kanban.view';

/** The last view used, so opening a project from the sidebar keeps it (server prefs in M3). */
export function lastView(): View {
  try {
    return localStorage.getItem(VIEW_KEY) === 'board' ? 'board' : 'list';
  } catch {
    return 'list';
  }
}

function parse(hash: string): Route {
  const m = /^#\/p\/(\d+)(?:\/(list|board))?/.exec(hash);
  if (!m) return { name: 'home' };
  return { name: 'project', id: Number(m[1]), view: (m[2] as View | undefined) ?? lastView() };
}

export const router = $state<{ route: Route }>({ route: parse(location.hash) });
window.addEventListener('hashchange', () => (router.route = parse(location.hash)));

export function go(route: { name: 'home' } | { name: 'project'; id: number; view?: View }) {
  if (route.name === 'home') {
    location.hash = '#/';
    return;
  }
  const view = route.view ?? lastView();
  try {
    localStorage.setItem(VIEW_KEY, view);
  } catch {
    /* not remembered; fine */
  }
  location.hash = `#/p/${route.id}/${view}`;
}
