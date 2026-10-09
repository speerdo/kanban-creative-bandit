// App-wide state: who's signed in, the people and projects, toasts, and the current route.

import { api, ApiError, type Me, type Project, type User } from './api';

export const session = $state<{ me: Me | null; checked: boolean }>({ me: null, checked: false });
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

// ---- routing (hash based: #/p/3, #/p/3/board, #/settings) ---------------------------------

export type View = 'list' | 'board';
/** A project route without a view opens in the user's default view. */
export type Route = { name: 'home' } | { name: 'settings' } | { name: 'project'; id: number; view?: View };

function parse(hash: string): Route {
  if (hash.startsWith('#/settings')) return { name: 'settings' };
  const m = /^#\/p\/(\d+)(?:\/(list|board))?/.exec(hash);
  if (!m) return { name: 'home' };
  return { name: 'project', id: Number(m[1]), view: m[2] as View | undefined };
}

export const router = $state<{ route: Route }>({ route: parse(location.hash) });
window.addEventListener('hashchange', () => (router.route = parse(location.hash)));

export function defaultView(): View {
  return session.me?.prefs.default_view ?? 'list';
}

export function go(route: Route) {
  if (route.name === 'project') location.hash = route.view ? `#/p/${route.id}/${route.view}` : `#/p/${route.id}`;
  else location.hash = route.name === 'settings' ? '#/settings' : '#/';
}
