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

// ---- routing (hash based: #/p/3) -----------------------------------------------------------

export type Route = { name: 'home' } | { name: 'project'; id: number };

function parse(hash: string): Route {
  const m = /^#\/p\/(\d+)/.exec(hash);
  return m ? { name: 'project', id: Number(m[1]) } : { name: 'home' };
}

export const router = $state<{ route: Route }>({ route: parse(location.hash) });
window.addEventListener('hashchange', () => (router.route = parse(location.hash)));

export function go(route: Route) {
  location.hash = route.name === 'project' ? `#/p/${route.id}` : '#/';
}
