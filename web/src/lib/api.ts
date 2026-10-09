// Typed client for the Rust API. Types mirror the server's JSON (server/src/routes, auth.rs).

export type Health = {
  status: 'ok' | 'degraded';
  version: string;
  db: 'ok' | 'error';
};

export type User = {
  id: number;
  username: string;
  display_name: string;
  avatar_color: string;
};

export type Prefs = {
  theme: 'system' | 'light' | 'dark';
  /** Palette name or #rrggbb. */
  accent_color: string;
  /** Palette name, #rrggbb, or null for the theme's own canvas. */
  background_color: string | null;
  background_style: 'solid' | 'gradient' | 'subtle-pattern';
  density: 'comfortable' | 'compact';
  default_view: 'list' | 'board';
};

/** The signed-in user, with their prefs. */
export type Me = User & { prefs: Prefs };

export type Project = {
  id: number;
  name: string;
  color: string;
  description: string;
  archived: boolean;
  position: string;
  created_by: number | null;
  created_at: string;
};

export type Category = 'todo' | 'in_progress' | 'done';

export type Status = {
  id: number;
  project_id: number;
  name: string;
  color: string;
  category: Category;
  position: string;
};

export const PRIORITIES = ['none', 'low', 'medium', 'high', 'urgent'] as const;
export type Priority = (typeof PRIORITIES)[number];

export type Task = {
  id: number;
  project_id: number;
  status_id: number;
  parent_task_id: number | null;
  title: string;
  description: string;
  assignee_id: number | null;
  priority: Priority;
  due_date: string | null;
  start_date: string | null;
  position: string;
  completed_at: string | null;
  created_by: number | null;
  created_at: string;
  updated_at: string;
  subtask_count: number;
  subtasks_done: number;
};

export type Placement = { after_id?: number; before_id?: number };

export type TaskPatch = Partial<
  Pick<Task, 'title' | 'description' | 'assignee_id' | 'priority' | 'due_date' | 'start_date' | 'status_id'>
> & { completed?: boolean };

export type NewTask = {
  project_id?: number;
  status_id?: number;
  parent_task_id?: number;
  title: string;
  description?: string;
  assignee_id?: number | null;
  priority?: Priority;
  due_date?: string | null;
} & Placement;

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

/** Called on any 401 so the app can drop back to the sign-in screen. */
let onUnauthorized: () => void = () => {};
export function setUnauthorizedHandler(fn: () => void) {
  onUnauthorized = fn;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: {
      Accept: 'application/json',
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (res.status === 204) return undefined as T;
  const data = await res.json().catch(() => null);
  if (!res.ok) {
    if (res.status === 401 && path !== '/auth/login') onUnauthorized();
    throw new ApiError(res.status, data?.error ?? `${res.status} ${res.statusText}`);
  }
  return data as T;
}

const get = <T>(path: string) => request<T>('GET', path);
const qs = (params: Record<string, string | number | boolean | undefined>) => {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined) p.set(k, String(v));
  const s = p.toString();
  return s ? `?${s}` : '';
};

export const api = {
  health: async () => {
    // 503 still carries a useful body.
    const res = await fetch('/api/health', { headers: { Accept: 'application/json' } });
    return (await res.json()) as Health;
  },

  login: (username: string, password: string) =>
    request<Me>('POST', '/auth/login', { username, password }),
  logout: () => request<void>('POST', '/auth/logout'),
  me: () => get<Me>('/me'),
  updatePrefs: (p: Partial<Prefs>) => request<Prefs>('PUT', '/me/prefs', p),
  updateProfile: (p: { display_name?: string; avatar_color?: string }) => request<Me>('PATCH', '/me', p),
  users: () => get<User[]>('/users'),

  projects: (archived = false) => get<Project[]>(`/projects${qs({ archived: archived || undefined })}`),
  createProject: (p: { name: string; color?: string }) => request<Project>('POST', '/projects', p),
  updateProject: (id: number, p: Partial<Pick<Project, 'name' | 'color' | 'description' | 'archived'>> & Placement) =>
    request<Project>('PATCH', `/projects/${id}`, p),
  deleteProject: (id: number) => request<void>('DELETE', `/projects/${id}`),

  statuses: (projectId: number) => get<Status[]>(`/projects/${projectId}/statuses`),
  createStatus: (projectId: number, s: { name: string; color?: string; category?: Category } & Placement) =>
    request<Status>('POST', `/projects/${projectId}/statuses`, s),
  updateStatus: (id: number, s: Partial<Pick<Status, 'name' | 'color' | 'category'>> & Placement) =>
    request<Status>('PATCH', `/statuses/${id}`, s),
  deleteStatus: (id: number, moveTo?: number) =>
    request<void>('DELETE', `/statuses/${id}${qs({ move_to: moveTo })}`),

  tasks: (f: { project?: number; assignee?: string; status?: number; parent?: number; q?: string } = {}) =>
    get<Task[]>(`/tasks${qs(f)}`),
  createTask: (t: NewTask) => request<Task>('POST', '/tasks', t),
  updateTask: (id: number, patch: TaskPatch) => request<Task>('PATCH', `/tasks/${id}`, patch),
  moveTask: (id: number, m: { status_id?: number } & Placement) => request<Task>('POST', `/tasks/${id}/move`, m),
  deleteTask: (id: number) => request<void>('DELETE', `/tasks/${id}`),
};
