// Run with `npm test` (Node's built-in runner; Node strips the TypeScript types).
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { parseDate, parseQuickAdd } from './quickadd.ts';

const users = [
  { id: 1, username: 'adam', display_name: 'Adam', avatar_color: 'violet' },
  { id: 2, username: 'kat', display_name: 'Kat', avatar_color: 'pink' },
];
const thursday = new Date(2026, 9, 8); // 2026-10-08

test('pulls priority, assignee and date out of the title', () => {
  const p = parseQuickAdd('Fix login !high @kat #bug fri', users, thursday);
  assert.deepEqual([p.title, p.priority, p.assignee?.id, p.due_date, p.labels], ['Fix login', 'high', 2, '2026-10-09', ['bug']]);
});

test('labels: letters first, deduplicated, numbers left alone', () => {
  const p = parseQuickAdd('Fix #2 crash #Bug #bug #to-buy', users, thursday);
  assert.deepEqual([p.title, p.labels], ['Fix #2 crash', ['Bug', 'to-buy']]);
});

test('dates only count at the end of the line', () => {
  assert.equal(parseQuickAdd("Today's standup notes", users, thursday).due_date, undefined);
  const p = parseQuickAdd('Buy sun cream sat', users, thursday);
  assert.deepEqual([p.title, p.due_date], ['Buy sun cream', '2026-10-10']);
  assert.deepEqual(parseQuickAdd('tomorrow', users, thursday).title, 'tomorrow');
});

test('unknown @names stay in the title', () => {
  const p = parseQuickAdd('pay rent @nobody 11/1 !1', users, thursday);
  assert.deepEqual([p.title, p.priority, p.due_date], ['pay rent @nobody', 'urgent', '2026-11-01']);
});

test('date words', () => {
  assert.equal(parseDate('thu', thursday), '2026-10-08');
  assert.equal(parseDate('wednesday', thursday), '2026-10-14');
  assert.equal(parseDate('tmr', thursday), '2026-10-09');
  assert.equal(parseDate('1/5', thursday), '2027-01-05');
  assert.equal(parseDate('31.12', thursday), '2026-12-31');
  assert.equal(parseDate('2/30', thursday), undefined);
  assert.equal(parseDate('we', thursday), undefined);
});
