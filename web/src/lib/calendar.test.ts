import assert from 'node:assert/strict';
import { test } from 'node:test';

import { addDays, eventDays, monthGrid, weekOf } from './calendar.ts';

test('weeks start on Monday', () => {
  // 2026-10-09 is a Friday.
  assert.deepEqual(weekOf('2026-10-09'), [
    '2026-10-05', '2026-10-06', '2026-10-07', '2026-10-08', '2026-10-09', '2026-10-10', '2026-10-11',
  ]);
  assert.equal(weekOf('2026-10-05')[0], '2026-10-05');
  assert.equal(weekOf('2026-10-11')[0], '2026-10-05');
});

test('month grid covers the month in whole weeks', () => {
  const oct = monthGrid('2026-10-20');
  assert.equal(oct.length, 5);
  assert.equal(oct[0][0], '2026-09-28');
  assert.equal(oct.at(-1)!.at(-1), '2026-11-01');
  // February 2027 starts on a Monday and fills exactly four weeks.
  const feb = monthGrid('2027-02-14');
  assert.equal(feb.length, 4);
  assert.equal(feb[0][0], '2027-02-01');
  // Never more than six rows.
  assert.ok(monthGrid('2026-08-01').length <= 6);
});

test('adding days crosses months, years and DST changes', () => {
  assert.equal(addDays('2026-10-31', 1), '2026-11-01');
  assert.equal(addDays('2026-12-31', 1), '2027-01-01');
  assert.equal(addDays('2026-11-01', 1), '2026-11-02'); // US DST ends
  assert.equal(addDays('2026-03-08', -1), '2026-03-07'); // US DST starts
});

test('event days', () => {
  // All-day ends are exclusive.
  assert.deepEqual(eventDays({ start_at: '2026-10-12', end_at: '2026-10-13', all_day: true }), ['2026-10-12', '2026-10-12']);
  assert.deepEqual(eventDays({ start_at: '2026-10-12', end_at: '2026-10-15', all_day: true }), ['2026-10-12', '2026-10-14']);
  // Timed events in local time; ending at midnight stays on the start day.
  const start = new Date(2026, 9, 21, 9, 0).toISOString();
  const midnight = new Date(2026, 9, 22, 0, 0).toISOString();
  assert.deepEqual(eventDays({ start_at: start, end_at: midnight, all_day: false }), ['2026-10-21', '2026-10-21']);
});
