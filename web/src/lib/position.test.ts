import assert from 'node:assert/strict';
import { test } from 'node:test';

import { between } from './position.ts';

// Expected values come from server/src/position.rs, so the two implementations agree.
test('matches the server', () => {
  assert.equal(between(null, null), 'V');
  assert.equal(between('V', null), 'l');
  assert.equal(between(null, 'V'), 'G');
  assert.equal(between('V', 'W'), 'VV');
  assert.equal(between('V', 'V1'), 'V0V');
});

test('stays strictly ordered under random inserts', () => {
  const keys = [between(null, null)];
  let seed = 7;
  const rand = (n: number) => ((seed = (seed * 1103515245 + 12345) % 2 ** 31), seed % n);
  for (let i = 0; i < 500; i++) {
    const at = rand(keys.length + 1);
    const k = between(keys[at - 1] ?? null, keys[at] ?? null);
    keys.splice(at, 0, k);
    assert.ok(!k.endsWith('0'), k);
  }
  for (let i = 1; i < keys.length; i++) assert.ok(keys[i - 1] < keys[i], `${keys[i - 1]} < ${keys[i]}`);
});
