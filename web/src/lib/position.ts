// Fractional-index keys, a direct port of server/src/position.rs. The client only uses it to
// guess where a dragged task lands so the UI reorders instantly; the server's answer wins.

const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
const BASE = DIGITS.length;

function digit(c: string): number {
  const d = DIGITS.indexOf(c);
  if (d < 0) throw new Error(`position key contains a non base-62 character: ${c}`);
  return d;
}

/** A key strictly between `a` and `b`; `null` means the start or end of the list. */
export function between(a: string | null, b: string | null): string {
  const lo = a ?? '';
  if (b !== null && !(lo < b)) throw new Error(`position keys out of order: ${lo} >= ${b}`);
  return midpoint(lo, b);
}

function midpoint(a: string, b: string | null): string {
  if (b !== null) {
    // Strip the common prefix (a missing digit in `a` counts as '0').
    let n = 0;
    while (n < b.length && (a[n] ?? '0') === b[n]) n++;
    if (n > 0) return b.slice(0, n) + midpoint(a.slice(n), b.slice(n));
  }
  const da = a.length > 0 ? digit(a[0]) : 0;
  const db = b !== null && b.length > 0 ? digit(b[0]) : BASE;
  if (db - da > 1) return DIGITS[Math.ceil((da + db) / 2)];
  // Leading digits are adjacent.
  if (b !== null && b.length > 1) return b[0];
  return DIGITS[da] + midpoint(a.slice(1), null);
}
