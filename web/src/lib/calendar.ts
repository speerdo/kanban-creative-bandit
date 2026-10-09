// Date arithmetic for the Calendar view. Days are local `YYYY-MM-DD` strings throughout;
// weeks start on Monday.

import type { OverlayEvent } from './api';

/** A local date as `YYYY-MM-DD` (same as quickadd's; repeated so this file has no imports to resolve under node --test). */
export function isoDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** A local date from `YYYY-MM-DD` (midnight local time, unlike `new Date(iso)` which is UTC). */
export function fromIso(day: string): Date {
  const [y, m, d] = day.split('-').map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(day: string, n: number): string {
  const d = fromIso(day);
  d.setDate(d.getDate() + n);
  return isoDate(d);
}

/** The Monday on or before `day`. */
export function weekStart(day: string): string {
  const dow = (fromIso(day).getDay() + 6) % 7; // Monday = 0
  return addDays(day, -dow);
}

/** Seven days from the Monday of `day`'s week. */
export function weekOf(day: string): string[] {
  const start = weekStart(day);
  return Array.from({ length: 7 }, (_, i) => addDays(start, i));
}

/** Whole weeks covering `day`'s month: 4 to 6 rows of 7 days. */
export function monthGrid(day: string): string[][] {
  const first = `${day.slice(0, 7)}-01`;
  const month = first.slice(0, 7);
  const weeks: string[][] = [];
  for (let start = weekStart(first); start.slice(0, 7) <= month || weeks.length === 0; start = addDays(start, 7)) {
    weeks.push(weekOf(start));
    if (weeks.length === 6) break;
  }
  return weeks;
}

/** The first and last local day an event covers. */
export function eventDays(e: Pick<OverlayEvent, 'start_at' | 'end_at' | 'all_day'>): [string, string] {
  if (e.all_day) {
    // The end date is exclusive.
    const last = addDays(e.end_at.slice(0, 10), -1);
    const first = e.start_at.slice(0, 10);
    return [first, last < first ? first : last];
  }
  const start = new Date(e.start_at);
  // An event ending exactly at midnight doesn't spill into the next day.
  const end = new Date(Math.max(start.getTime(), new Date(e.end_at).getTime() - 1));
  return [isoDate(start), isoDate(end)];
}

/** "9:30" style local start time, or '' for all-day events. */
export function startTime(e: Pick<OverlayEvent, 'start_at' | 'all_day'>): string {
  if (e.all_day) return '';
  return new Date(e.start_at).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}
