// One project's statuses and tasks, shared by the List and Board views. Every change is
// optimistic (applied locally first, rolled back if the server refuses), and live events from
// the other browser are applied with `apply()`.

import { api, type Category, type NewTask, type Status, type Task, type TaskPatch } from './api';
import { between } from './position';
import { failed, session, toast } from './state.svelte';

/** Header filters; applied on the client to the tasks already loaded. */
export type Filter = {
  q: string;
  /** 'any', 'me', 'none' (unassigned), or a user id. */
  assignee: 'any' | 'me' | 'none' | number;
  label: number | null;
  hideDone: boolean;
};

export const NO_FILTER: Filter = { q: '', assignee: 'any', label: null, hideDone: false };

const byPosition = <T extends { position: string; id: number }>(a: T, b: T) =>
  a.position < b.position ? -1 : a.position > b.position ? 1 : a.id - b.id;

/** A key that sorts after every real one, used until the server answers. */
const END = '~';

export class ProjectStore {
  statuses = $state<Status[]>([]);
  tasks = $state<Task[]>([]);
  loading = $state(true);
  filter = $state<Filter>({ ...NO_FILTER });

  filtering = $derived(
    this.filter.q.trim() !== '' || this.filter.assignee !== 'any' || this.filter.label !== null || this.filter.hideDone,
  );

  visible(t: Task): boolean {
    const f = this.filter;
    if (f.hideDone && t.completed_at) return false;
    if (f.label !== null && !t.label_ids.includes(f.label)) return false;
    if (f.assignee === 'none' && t.assignee_id !== null) return false;
    if (f.assignee === 'me' && t.assignee_id !== session.me?.id) return false;
    if (typeof f.assignee === 'number' && t.assignee_id !== f.assignee) return false;
    const q = f.q.trim().toLowerCase();
    if (q && !t.title.toLowerCase().includes(q) && !t.description.toLowerCase().includes(q)) return false;
    return true;
  }

  /** Visible top-level tasks per status, in display order. */
  byStatus = $derived.by(() => {
    const groups = new Map<number, Task[]>(this.statuses.map((s) => [s.id, []]));
    for (const t of this.tasks) if (this.visible(t)) groups.get(t.status_id)?.push(t);
    for (const list of groups.values()) list.sort(byPosition);
    return groups;
  });

  constructor(public readonly id: number) {}

  async load() {
    try {
      const [statuses, tasks] = await Promise.all([api.statuses(this.id), api.tasks({ project: this.id })]);
      this.statuses = statuses.sort(byPosition);
      this.tasks = tasks;
    } catch (e) {
      failed(e);
    } finally {
      this.loading = false;
    }
  }

  async reloadTasks() {
    try {
      this.tasks = await api.tasks({ project: this.id });
    } catch (e) {
      failed(e);
    }
  }

  categoryOf(statusId: number): Category | undefined {
    return this.statuses.find((s) => s.id === statusId)?.category;
  }

  // ---- tasks ---------------------------------------------------------------------------------

  async updateTask(task: Task, patch: TaskPatch) {
    const before = $state.snapshot(task);
    const { completed, ...local }: TaskPatch & Partial<Task> = { ...patch };
    // A status change lands at the end of the new column until the server answers.
    if (patch.status_id !== undefined && patch.status_id !== task.status_id) {
      local.position = END;
      local.completed_at = this.categoryOf(patch.status_id) === 'done' ? new Date().toISOString() : null;
    }
    // The checkbox: guess the done column; reopening is left to the server's answer.
    if (completed !== undefined) {
      local.completed_at = completed ? new Date().toISOString() : null;
      const done = this.statuses.find((s) => s.category === 'done');
      if (completed && done) Object.assign(local, { status_id: done.id, position: END });
    }
    Object.assign(task, local);
    try {
      Object.assign(task, await api.updateTask(task.id, patch));
    } catch (e) {
      Object.assign(task, before);
      failed(e);
    }
  }

  /** Drag and drop: put `task` at `index` in `statusId`'s list (index counted without it). */
  async moveTask(task: Task, statusId: number, index: number) {
    const list = this.byStatus.get(statusId) ?? [];
    // Dropped back where it was: nothing to do.
    if (task.status_id === statusId && list.indexOf(task) === index) return;
    const others = list.filter((t) => t.id !== task.id);
    const prev = others[index - 1];
    const next = others[index];

    const before = $state.snapshot(task);
    let guess: string;
    try {
      guess = between(prev?.position ?? null, next?.position ?? null);
    } catch {
      guess = END; // neighbours collided; the server sorts it out
    }
    task.status_id = statusId;
    task.position = guess;
    task.completed_at = this.categoryOf(statusId) === 'done' ? (task.completed_at ?? new Date().toISOString()) : null;

    try {
      const placement = prev ? { after_id: prev.id } : next ? { before_id: next.id } : {};
      Object.assign(task, await api.moveTask(task.id, { status_id: statusId, ...placement }));
    } catch (e) {
      Object.assign(task, before);
      failed(e);
    }
  }

  async createTask(t: NewTask): Promise<boolean> {
    try {
      this.upsertTask(await api.createTask({ project_id: this.id, ...t }));
      return true;
    } catch (e) {
      return failed(e);
    }
  }

  async deleteTask(task: Task) {
    const i = this.tasks.findIndex((t) => t.id === task.id);
    if (i < 0) return;
    this.tasks.splice(i, 1);
    try {
      await api.deleteTask(task.id);
      toast(`Deleted “${task.title}”`);
    } catch (e) {
      this.tasks.splice(i, 0, task);
      failed(e);
    }
  }

  // ---- statuses ------------------------------------------------------------------------------

  async updateStatus(status: Status, patch: Partial<Pick<Status, 'name' | 'color' | 'category'>>) {
    const before = $state.snapshot(status);
    Object.assign(status, patch);
    try {
      Object.assign(status, await api.updateStatus(status.id, patch));
      if (patch.category) await this.reloadTasks(); // completion changed
    } catch (e) {
      Object.assign(status, before);
      failed(e);
    }
  }

  async addStatus(after: Status) {
    try {
      this.upsertStatus(await api.createStatus(this.id, { name: 'New section', color: 'slate', after_id: after.id }));
    } catch (e) {
      failed(e);
    }
  }

  async deleteStatus(status: Status, moveTo?: number) {
    try {
      await api.deleteStatus(status.id, moveTo);
      this.removeStatus(status.id);
      if (moveTo !== undefined) await this.reloadTasks();
    } catch (e) {
      failed(e);
    }
  }

  // ---- live events ---------------------------------------------------------------------------

  /** Applies a server event; anything for another project is ignored. */
  apply(kind: string, data: Record<string, unknown>) {
    if (data.project_id !== this.id) return;
    switch (kind) {
      case 'task.created':
      case 'task.updated':
      case 'task.moved':
        // Subtasks aren't listed; their parent's counts arrive as their own event.
        if (data.parent_task_id === null) this.upsertTask(data as unknown as Task);
        break;
      case 'task.deleted':
        this.tasks = this.tasks.filter((t) => t.id !== data.id);
        break;
      case 'status.created':
      case 'status.updated':
        this.upsertStatus(data as unknown as Status);
        break;
      case 'status.deleted':
        this.removeStatus(data.id as number);
        break;
      case 'project.tasks_changed':
        this.reloadTasks();
        break;
    }
  }

  private upsertTask(task: Task) {
    const existing = this.tasks.find((t) => t.id === task.id);
    if (existing) Object.assign(existing, task);
    else this.tasks.push(task);
  }

  private upsertStatus(status: Status) {
    const existing = this.statuses.find((s) => s.id === status.id);
    if (existing) Object.assign(existing, status);
    else this.statuses.push(status);
    this.statuses.sort(byPosition);
  }

  private removeStatus(id: number) {
    this.statuses = this.statuses.filter((s) => s.id !== id);
  }
}

/** The project on screen, so live events can reach it. */
export const active = $state<{ store: ProjectStore | null }>({ store: null });
