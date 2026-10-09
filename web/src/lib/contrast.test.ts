import assert from 'node:assert/strict';
import { test } from 'node:test';

import { contrast, DARK_TEXT, hexToRgb, LIGHT_TEXT, mutedOn, textOn } from './contrast.ts';

test('contrast ratio matches WCAG reference values', () => {
  assert.equal(contrast([0, 0, 0], [255, 255, 255]).toFixed(2), '21.00');
  assert.equal(contrast([119, 119, 119], [255, 255, 255]).toFixed(2), '4.48');
});

test('picks readable text for the brand colors', () => {
  assert.equal(textOn(hexToRgb('#ffe800')), DARK_TEXT); // acid yellow
  assert.equal(textOn(hexToRgb('#1b27e8')), LIGHT_TEXT); // cold blue
  assert.equal(textOn(hexToRgb('#ede8df')), DARK_TEXT); // paper
  assert.equal(textOn(hexToRgb('#0b0b0c')), LIGHT_TEXT); // base
});

test('every pick meets AA for large text, and the better one wins', () => {
  for (let i = 0; i < 2000; i++) {
    const bg: [number, number, number] = [(i * 37) % 256, (i * 91) % 256, (i * 53) % 256];
    const pick = hexToRgb(textOn(bg));
    const other = hexToRgb(textOn(bg) === DARK_TEXT ? LIGHT_TEXT : DARK_TEXT);
    assert.ok(contrast(pick, bg) >= contrast(other, bg));
    assert.ok(contrast(pick, bg) >= 3, `${bg} → ${contrast(pick, bg)}`);
  }
});

test('muted text still reaches 4.5:1', () => {
  for (const hex of ['#ede8df', '#0b0b0c', '#1b27e8', '#ffe800', '#5a8f6b']) {
    const bg = hexToRgb(hex);
    const [r, g, b] = mutedOn(bg).match(/\d+/g)!.map(Number);
    assert.ok(contrast([r, g, b], bg) >= 4.5, hex);
  }
});
