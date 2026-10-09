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
  label_ids: number[];
};

export type Label = { id: number; name: string; color: string };

export type Comment = {
  id: number;
  task_id: number;
  project_id: number;
  author_id: number | null;
  body: string;
  created_at: string;
  edited_at: string | null;
};

export type Activity = {
  id: number;
  task_id: number;
  actor_id: number | null;
  kind: string;
  from_value: string | null;
  to_value: string | null;
  created_at: string;
};

export type Placement = { after_id?: number; before_id?: number };

// ---- Lists and links (server/src/routes/lists.rs, links.rs) ----------------------------------

export type ShoppingList = {
  id: number;
  name: string;
  color: string;
  position: string;
  created_by: number | null;
  created_at: string;
  open_count: number;
};

export type ListItem = {
  id: number;
  list_id: number;
  text: string;
  note: string;
  position: string;
  checked_at: string | null;
  checked_by: number | null;
  created_by: number | null;
  created_at: string;
};

export type LinkKind = 'gmail' | 'doc' | 'sheet' | 'slides' | 'form' | 'drive' | 'url';

export type TaskLink = {
  id: number;
  task_id: number;
  kind: LinkKind;
  external_id: string | null;
  url: string;
  title: string | null;
  mime_type: string | null;
  added_by: number | null;
  created_at: string;
};

/** A Keep note from a Takeout export, as sent to the import. */
export type KeepNote = {
  title: string;
  text: string;
  items: { text: string; checked: boolean }[] | null;
  archived: boolean;
  trashed: boolean;
};

export type KeepReport = {
  lists: { name: string; items: number; merged: boolean }[];
  tasks: string[];
  skipped: number;
  dry_run: boolean;
};

export type GoogleFeature = 'gmail' | 'drive';

// ---- Google (server/src/google) ---------------------------------------------------------------

export type ChosenCalendar = {
  google_calendar_id: string;
  summary: string;
  color: string | null;
  role: 'tasks' | 'overlay';
  synced_at: string | null;
};

export type GoogleStatus = {
  /** Google is set up on the server. */
  configured: boolean;
  connected: boolean;
  email: string | null;
  connected_at: string | null;
  last_sync_at: string | null;
  last_push_at: string | null;
  last_error: string | null;
  calendars: ChosenCalendar[];
  /** Changes the next Push to Google would send. */
  pending: number;
  redirect_uri: string;
  /** Optional access granted. */
  features: GoogleFeature[];
  gmail_enabled: boolean;
  gmail_project_id: number | null;
};

export type CalendarOption = {
  id: string;
  summary: string;
  color: string | null;
  primary: boolean;
  writable: boolean;
  role: 'tasks' | 'overlay' | null;
};

export type PushReport = { created: number; updated: number; deleted: number };

/** A read-only event from one of my Google calendars. */
export type OverlayEvent = {
  google_event_id: string;
  calendar: string;
  color: string | null;
  summary: string;
  /** YYYY-MM-DD when all_day (end exclusive), else RFC 3339. */
  start_at: string;
  end_at: string;
  all_day: boolean;
  html_link: string | null;
  location: string | null;
};

export type TaskPatch = Partial<
  Pick<Task, 'title' | 'description' | 'assignee_id' | 'priority' | 'due_date' | 'start_date' | 'status_id' | 'label_ids'>
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
  label_ids?: number[];
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

  allStatuses: () => get<Status[]>('/statuses'),

  tasks: (
    f: {
      project?: number;
      assignee?: string;
      status?: number;
      parent?: number;
      label?: number;
      q?: string;
      completed?: boolean;
      any_level?: boolean;
    } = {},
  ) => get<Task[]>(`/tasks${qs(f)}`),
  task: (id: number) => get<Task>(`/tasks/${id}`),
  createTask: (t: NewTask) => request<Task>('POST', '/tasks', t),
  updateTask: (id: number, patch: TaskPatch) => request<Task>('PATCH', `/tasks/${id}`, patch),
  moveTask: (id: number, m: { status_id?: number } & Placement) => request<Task>('POST', `/tasks/${id}/move`, m),
  deleteTask: (id: number) => request<void>('DELETE', `/tasks/${id}`),
  activity: (taskId: number) => get<Activity[]>(`/tasks/${taskId}/activity`),

  labels: () => get<Label[]>('/labels'),
  createLabel: (l: { name: string; color?: string }) => request<Label>('POST', '/labels', l),
  updateLabel: (id: number, l: { name?: string; color?: string }) => request<Label>('PATCH', `/labels/${id}`, l),
  deleteLabel: (id: number) => request<void>('DELETE', `/labels/${id}`),

  calendar: (from: string, to: string) =>
    get<{ tasks: Task[]; events: OverlayEvent[] }>(`/calendar${qs({ from, to })}`),

  google: () => get<GoogleStatus>('/integrations/google'),
  googleStart: (features: GoogleFeature[] = []) =>
    request<{ auth_url: string }>('POST', '/integrations/google/start', { features }),
  googleGmail: (g: { enabled: boolean; project_id?: number | null }) =>
    request<GoogleStatus>('PUT', '/integrations/google/gmail', g),
  googleFinish: (redirected_url: string) =>
    request<GoogleStatus>('POST', '/integrations/google/finish', { redirected_url }),
  googleDisconnect: () => request<void>('DELETE', '/integrations/google'),
  googleCalendars: () => get<CalendarOption[]>('/integrations/google/calendars'),
  googleChooseCalendars: (c: { tasks: string | null; overlays: string[] }) =>
    request<GoogleStatus>('PUT', '/integrations/google/calendars', c),
  googleSync: () => request<{ status: GoogleStatus }>('POST', '/integrations/google/sync'),
  googlePending: () => get<{ pending: number }>('/integrations/google/push'),
  googlePush: () => request<{ report: PushReport; status: GoogleStatus }>('POST', '/integrations/google/push'),

  lists: () => get<ShoppingList[]>('/lists'),
  createList: (l: { name: string; color?: string }) => request<ShoppingList>('POST', '/lists', l),
  updateList: (id: number, l: { name?: string; color?: string } & Placement) =>
    request<ShoppingList>('PATCH', `/lists/${id}`, l),
  deleteList: (id: number) => request<void>('DELETE', `/lists/${id}`),
  listItems: (id: number) => get<ListItem[]>(`/lists/${id}/items`),
  addListItems: (id: number, text: string) => request<ListItem[]>('POST', `/lists/${id}/items`, { text }),
  updateListItem: (id: number, p: { text?: string; note?: string; checked?: boolean } & Placement) =>
    request<ListItem>('PATCH', `/list-items/${id}`, p),
  deleteListItem: (id: number) => request<void>('DELETE', `/list-items/${id}`),
  clearChecked: (id: number) => request<{ removed: number }>('POST', `/lists/${id}/clear-checked`),

  links: (taskId: number) => get<TaskLink[]>(`/tasks/${taskId}/links`),
  addLink: (taskId: number, url: string, title?: string) =>
    request<TaskLink>('POST', `/tasks/${taskId}/links`, { url, title }),
  deleteLink: (taskId: number, id: number) => request<void>('DELETE', `/tasks/${taskId}/links/${id}`),

  importKeep: (b: { notes: KeepNote[]; project_id: number | null; include_archived: boolean; dry_run: boolean }) =>
    request<KeepReport>('POST', '/import/keep', b),

  comments: (taskId: number) => get<Comment[]>(`/tasks/${taskId}/comments`),
  createComment: (taskId: number, body: string) => request<Comment>('POST', `/tasks/${taskId}/comments`, { body }),
  updateComment: (id: number, body: string) => request<Comment>('PATCH', `/comments/${id}`, { body }),
  deleteComment: (id: number) => request<void>('DELETE', `/comments/${id}`),
};
