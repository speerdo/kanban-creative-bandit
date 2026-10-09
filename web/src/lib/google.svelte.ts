// My Google connection, shared by Settings and the Calendar view: status, Sync now, and
// Push to Google. Nothing is written to Google except by `push()`.

import { api, type GoogleStatus } from './api';
import { failed, toast } from './state.svelte';

export const google = $state<{ status: GoogleStatus | null; syncing: boolean; pushing: boolean }>({
  status: null,
  syncing: false,
  pushing: false,
});

export async function loadGoogle() {
  try {
    google.status = await api.google();
  } catch (e) {
    failed(e);
  }
}

/** Refreshes only the "Push to Google (n)" count, e.g. after a task changed. */
let countTimer: ReturnType<typeof setTimeout> | undefined;
export function refreshPending() {
  if (!google.status?.connected) return;
  clearTimeout(countTimer);
  countTimer = setTimeout(async () => {
    try {
      const { pending } = await api.googlePending();
      if (google.status) google.status.pending = pending;
    } catch {
      // The count is a hint; the next full load corrects it.
    }
  }, 400);
}

/** Pulls from Google now. `quiet` skips the toast (used when the Calendar view opens). */
export async function syncNow(quiet = false) {
  if (google.syncing) return;
  google.syncing = true;
  try {
    google.status = (await api.googleSync()).status;
    if (!quiet) toast('Synced with Google');
  } catch (e) {
    if (!quiet) failed(e);
    await loadGoogle();
  } finally {
    google.syncing = false;
  }
}

export async function push() {
  if (google.pushing) return;
  google.pushing = true;
  try {
    const { report, status } = await api.googlePush();
    google.status = status;
    const parts = [
      report.created && `${report.created} added`,
      report.updated && `${report.updated} updated`,
      report.deleted && `${report.deleted} removed`,
    ].filter(Boolean);
    toast(parts.length ? `Google Calendar: ${parts.join(', ')}` : 'Google Calendar is already up to date');
  } catch (e) {
    failed(e);
    await loadGoogle();
  } finally {
    google.pushing = false;
  }
}

/** "just now", "5 min ago", "3 h ago", or a date. */
export function since(iso: string | null): string {
  if (!iso) return 'never';
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins} min ago`;
  if (mins < 24 * 60) return `${Math.round(mins / 60)} h ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
