// Keyboard shortcuts (blueprint §5). They act on the selected task: the one last hovered or
// reached with J/K. Status changes need the project's columns, so they work in project views.

import { active } from './project.svelte';
import { openTask, router, selection } from './state.svelte';

export const SHORTCUTS: [string, string][] = [
  ['Q', 'Quick add a task'],
  ['/', 'Search this project'],
  ['J / K', 'Select next / previous task'],
  ['Enter', 'Open the selected task'],
  ['Esc', 'Close the task panel'],
  ['1 – 9', 'Move the selected task to that column'],
  ['[ / ]', 'Move it one column left / right'],
  ['Ctrl/⌘ + Enter', 'Complete or reopen it'],
  ['?', 'Show these shortcuts'],
];

function typingIn(el: EventTarget | null): boolean {
  const e = el as HTMLElement | null;
  return !!e && (e.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(e.tagName));
}

/** Visible task rows/cards in page order (the detail panel's own list doesn't count). */
function taskElements(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('main [data-task]')];
}

function step(delta: 1 | -1) {
  const els = taskElements();
  if (els.length === 0) return;
  const i = els.findIndex((el) => Number(el.dataset.task) === selection.taskId);
  const next = els[i < 0 ? 0 : Math.min(els.length - 1, Math.max(0, i + delta))];
  selection.taskId = Number(next.dataset.task);
  next.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

/** Handles a keydown; returns true when it was a shortcut. `onQuickAdd`/`onHelp` open dialogs. */
export function handleShortcut(e: KeyboardEvent, onQuickAdd: () => void, onHelp: () => void): boolean {
  if (e.defaultPrevented) return false;
  if (typingIn(e.target)) {
    // Esc leaves the field (saving it, as fields save on blur); a second Esc closes the panel.
    if (e.key === 'Escape') {
      (e.target as HTMLElement).blur();
      return true;
    }
    return false;
  }
  const store = active.store;
  const task = store?.tasks.find((t) => t.id === selection.taskId);

  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    if (store && task) store.updateTask(task, { completed: !task.completed_at });
    return !!task;
  }
  if (e.ctrlKey || e.metaKey || e.altKey) return false;

  switch (e.key) {
    case 'q':
    case 'Q':
      onQuickAdd();
      return true;
    case '?':
      onHelp();
      return true;
    case '/': {
      const search = document.getElementById('task-search') as HTMLInputElement | null;
      search?.focus();
      return !!search;
    }
    case 'j':
    case 'J':
      step(1);
      return true;
    case 'k':
    case 'K':
      step(-1);
      return true;
    case 'Enter':
      if (selection.taskId === null) return false;
      openTask(selection.taskId);
      return true;
    case 'Escape': {
      const r = router.route;
      if ((r.name === 'project' || r.name === 'my') && r.taskId) {
        openTask(null);
        return true;
      }
      return false;
    }
    case '[':
    case ']': {
      if (!store || !task) return false;
      const i = store.statuses.findIndex((s) => s.id === task.status_id);
      const next = store.statuses[i + (e.key === ']' ? 1 : -1)];
      if (next) store.updateTask(task, { status_id: next.id });
      return true;
    }
  }
  if (/^[1-9]$/.test(e.key) && store && task) {
    const target = store.statuses[Number(e.key) - 1];
    if (target && target.id !== task.status_id) store.updateTask(task, { status_id: target.id });
    return true;
  }
  return false;
}
