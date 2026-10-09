// App-wide state: who's signed in, the people and projects, toasts, and the current route.

import { api, ApiError, type Label, type Me, type Project, type ShoppingList, type User } from './api';

export const session = $state<{ me: Me | null; checked: boolean }>({ me: null, checked: false });
export const people = $state<{ users: User[] }>({ users: [] });
export const workspace = $state<{ projects: Project[]; labels: Label[]; lists: ShoppingList[] }>({
  projects: [],
  labels: [],
  lists: [],
});

export function userById(id: number | null | undefined): User | undefined {
  return id == null ? undefined : people.users.find((u) => u.id === id);
}

export function labelById(id: number): Label | undefined {
  return workspace.labels.find((l) => l.id === id);
}

export async function loadWorkspace() {
  const [users, projects, labels, lists] = await Promise.all([api.users(), api.projects(), api.labels(), api.lists()]);
  people.users = users;
  workspace.projects = projects;
  workspace.labels = labels;
  workspace.lists = lists;
}

/** Labels by name for quick add; unknown names are created (shared, so both of us get them). */
export async function labelIdsFor(names: string[]): Promise<number[]> {
  const ids: number[] = [];
  for (const name of names) {
    let label = workspace.labels.find((l) => l.name.toLowerCase() === name.toLowerCase());
    if (!label) {
      label = await api.createLabel({ name, color: 'slate' });
      if (!workspace.labels.some((l) => l.id === label!.id)) workspace.labels.push(label);
    }
    ids.push(label.id);
  }
  return ids;
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

// ---- routing --------------------------------------------------------------------------------
//
//   #/p/3  #/p/3/board     a project (no view = the user's default view)
//   #/my                   My Tasks
//   #/calendar             Calendar
//   #/l/2                  a shared list
//   #/settings
//   …/t/42                 on a project, My Tasks or Calendar: task 42 open in the detail panel

export type View = 'list' | 'board';
export type Route =
  | { name: 'home' }
  | { name: 'settings' }
  | { name: 'my'; taskId?: number }
  | { name: 'calendar'; taskId?: number }
  | { name: 'list'; id: number }
  | { name: 'project'; id: number; view?: View; taskId?: number };

function parse(hash: string): Route {
  if (hash.startsWith('#/settings')) return { name: 'settings' };
  const task = /\/t\/(\d+)/.exec(hash);
  const taskId = task ? Number(task[1]) : undefined;
  if (hash.startsWith('#/my')) return { name: 'my', taskId };
  if (hash.startsWith('#/calendar')) return { name: 'calendar', taskId };
  // `#/lists` (the home-screen start page) means "the list I had open last".
  if (hash.startsWith('#/lists')) return { name: 'list', id: 0 };
  const list = /^#\/l\/(\d+)/.exec(hash);
  if (list) return { name: 'list', id: Number(list[1]) };
  const m = /^#\/p\/(\d+)(?:\/(list|board))?/.exec(hash);
  if (!m) return { name: 'home' };
  return { name: 'project', id: Number(m[1]), view: m[2] as View | undefined, taskId };
}

function format(route: Route): string {
  const t = 'taskId' in route && route.taskId ? `/t/${route.taskId}` : '';
  switch (route.name) {
    case 'home':
      return '#/';
    case 'settings':
      return '#/settings';
    case 'my':
      return `#/my${t}`;
    case 'calendar':
      return `#/calendar${t}`;
    case 'list':
      return `#/l/${route.id}`;
    case 'project':
      return `#/p/${route.id}${route.view ? `/${route.view}` : t ? `/${defaultView()}` : ''}${t}`;
  }
}

export const router = $state<{ route: Route }>({ route: parse(location.hash) });
window.addEventListener('hashchange', () => (router.route = parse(location.hash)));

export function go(route: Route) {
  location.hash = format(route);
}

/** Opens (or with `null`, closes) the detail panel over the current page. */
export function openTask(taskId: number | null) {
  const r = router.route;
  if (r.name === 'project' || r.name === 'my' || r.name === 'calendar') go({ ...r, taskId: taskId ?? undefined });
}

export function defaultView(): View {
  return session.me?.prefs.default_view ?? 'list';
}


// ---- selection (keyboard shortcuts act on this task) ------------------------------------------

export const selection = $state<{ taskId: number | null }>({ taskId: null });
