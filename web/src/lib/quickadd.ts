// Parses one-line quick-add input, e.g. `Fix login !high @kat fri`.
//
//   !low !medium (!med) !high !urgent, or !1..!4 (1 = urgent)   priority
//   @name                                                       assignee (username or display-name prefix)
//   today tod tomorrow tmr, mon..sun / monday..sunday           due date (next occurrence, today counts)
//   2026-10-31, 10/31, 31.10                                     due date
//
// Dates are only read at the end of the line. Recognised tokens are removed from the title;
// anything else stays. `#labels` stay in the
// title until labels land (M4).

import type { Priority, User } from './api';

export type Parsed = {
  title: string;
  priority?: Priority;
  assignee?: User;
  due_date?: string;
};

const PRIORITY: Record<string, Priority> = {
  low: 'low',
  medium: 'medium',
  med: 'medium',
  high: 'high',
  urgent: 'urgent',
  '1': 'urgent',
  '2': 'high',
  '3': 'medium',
  '4': 'low',
};

const WEEKDAYS = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];

export function isoDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function addDays(d: Date, n: number): Date {
  const out = new Date(d);
  out.setDate(out.getDate() + n);
  return out;
}

/** A date word or number → YYYY-MM-DD, or undefined if it isn't one. */
export function parseDate(word: string, today = new Date()): string | undefined {
  const w = word.toLowerCase();
  if (w === 'today' || w === 'tod') return isoDate(today);
  if (w === 'tomorrow' || w === 'tmr') return isoDate(addDays(today, 1));

  // "fri", "frid", "friday" all work; at least three letters.
  const day = w.length >= 3 ? WEEKDAYS.findIndex((d) => d.startsWith(w)) : -1;
  if (day >= 0) return isoDate(addDays(today, (day - today.getDay() + 7) % 7));

  let m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(w);
  if (m) return valid(+m[1], +m[2], +m[3]);
  // m/d (US style) and d.m (European style); the year is this year, or next if already past.
  m = /^(\d{1,2})\/(\d{1,2})$/.exec(w);
  if (m) return upcoming(+m[1], +m[2], today);
  m = /^(\d{1,2})\.(\d{1,2})$/.exec(w);
  if (m) return upcoming(+m[2], +m[1], today);
  return undefined;
}

function valid(y: number, m: number, d: number): string | undefined {
  const date = new Date(y, m - 1, d);
  return date.getMonth() === m - 1 && date.getDate() === d ? isoDate(date) : undefined;
}

function upcoming(month: number, day: number, today: Date): string | undefined {
  const thisYear = valid(today.getFullYear(), month, day);
  if (!thisYear) return undefined;
  return thisYear >= isoDate(today) ? thisYear : valid(today.getFullYear() + 1, month, day);
}

export function findUser(name: string, users: User[]): User | undefined {
  const n = name.toLowerCase();
  return (
    users.find((u) => u.username === n) ??
    users.find((u) => u.username.startsWith(n) || u.display_name.toLowerCase().startsWith(n))
  );
}

export function parseQuickAdd(input: string, users: User[], today = new Date()): Parsed {
  const out: Parsed = { title: '' };
  const words = input.trim().split(/\s+/).filter(Boolean);
  const priorityOf = (w: string) => (w.length > 1 && w.startsWith('!') ? PRIORITY[w.slice(1).toLowerCase()] : undefined);
  const userOf = (w: string) => (w.length > 1 && w.startsWith('@') ? findUser(w.slice(1), users) : undefined);

  // A date only counts in the trailing run of tokens ("Buy sun cream" keeps its "sun"),
  // and never as the first word ("Today's standup").
  let tail = words.length;
  while (tail > 1 && (priorityOf(words[tail - 1]) || userOf(words[tail - 1]) || words[tail - 1].startsWith('#') || parseDate(words[tail - 1], today))) {
    tail--;
  }

  const kept: string[] = [];
  words.forEach((word, i) => {
    const priority = priorityOf(word);
    if (priority) return void (out.priority = priority);
    const user = userOf(word);
    if (user) return void (out.assignee = user);
    const date = i >= tail && !out.due_date ? parseDate(word, today) : undefined;
    if (date) return void (out.due_date = date);
    kept.push(word);
  });
  out.title = kept.join(' ');
  return out;
}
