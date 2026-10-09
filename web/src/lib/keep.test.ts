import assert from 'node:assert/strict';
import { test } from 'node:test';

import { toNote, unzip } from './keep.ts';

/** A minimal zip: one stored and one deflated entry, as Takeout might write. */
async function makeZip(files: [string, string, boolean][]): Promise<Uint8Array> {
  const enc = new TextEncoder();
  const parts: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  for (const [name, text, deflate] of files) {
    const raw = enc.encode(text);
    const data = deflate
      ? new Uint8Array(await new Response(new Blob([raw]).stream().pipeThrough(new CompressionStream('deflate-raw'))).arrayBuffer())
      : raw;
    const n = enc.encode(name);
    const local = new Uint8Array(30 + n.length);
    const lv = new DataView(local.buffer);
    lv.setUint32(0, 0x04034b50, true);
    lv.setUint16(8, deflate ? 8 : 0, true);
    lv.setUint32(18, data.length, true);
    lv.setUint32(22, raw.length, true);
    lv.setUint16(26, n.length, true);
    local.set(n, 30);
    const c = new Uint8Array(46 + n.length);
    const cv = new DataView(c.buffer);
    cv.setUint32(0, 0x02014b50, true);
    cv.setUint16(10, deflate ? 8 : 0, true);
    cv.setUint32(20, data.length, true);
    cv.setUint32(24, raw.length, true);
    cv.setUint16(28, n.length, true);
    cv.setUint32(42, offset, true);
    c.set(n, 46);
    parts.push(local, data);
    central.push(c);
    offset += local.length + data.length;
  }
  const cdSize = central.reduce((s, c) => s + c.length, 0);
  const end = new Uint8Array(22);
  const ev = new DataView(end.buffer);
  ev.setUint32(0, 0x06054b50, true);
  ev.setUint16(8, files.length, true);
  ev.setUint16(10, files.length, true);
  ev.setUint32(12, cdSize, true);
  ev.setUint32(16, offset, true);
  const all = [...parts, ...central, end];
  const out = new Uint8Array(all.reduce((s, p) => s + p.length, 0));
  let at = 0;
  for (const p of all) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

test('reads stored and deflated entries, only the ones asked for', async () => {
  const zip = await makeZip([
    ['Takeout/Keep/Groceries.json', '{"title":"Groceries"}', false],
    ['Takeout/Keep/Ideas.json', '{"title":"Ideas","textContent":"' + 'long '.repeat(200) + '"}', true],
    ['Takeout/Keep/Groceries.html', '<html>', false],
  ]);
  const entries = await unzip(zip, (n) => n.endsWith('.json'));
  assert.deepEqual([...entries.keys()], ['Takeout/Keep/Groceries.json', 'Takeout/Keep/Ideas.json']);
  const ideas = JSON.parse(new TextDecoder().decode(entries.get('Takeout/Keep/Ideas.json')));
  assert.equal(ideas.textContent.length, 1000);
  await assert.rejects(unzip(new TextEncoder().encode('not a zip at all, sorry'), () => true));
});

test('takeout notes become import notes', () => {
  assert.deepEqual(
    toNote({ title: ' Groceries ', listContent: [{ text: 'Milk', isChecked: false }, { text: ' ' }, { text: 'Rice', isChecked: true }], isArchived: false }),
    { title: 'Groceries', text: '', items: [{ text: 'Milk', checked: false }, { text: 'Rice', checked: true }], archived: false, trashed: false },
  );
  assert.equal(toNote({ textContent: 'hi', isTrashed: true }).items, null);
  assert.equal(toNote({ textContent: 'hi', isTrashed: true }).trashed, true);
});
