// Mirrors server/src/validate.rs COLORS; the CSS tokens live in app.css.
export const COLORS = [
  'slate', 'red', 'orange', 'amber', 'yellow', 'lime', 'green', 'teal', 'cyan', 'blue', 'violet', 'pink',
] as const;
export type Color = (typeof COLORS)[number];

// Every helper takes a palette name or a custom #rrggbb.
const isHex = (c: string) => c.startsWith('#');

/** Solid hue (dots, bars, stripes). Custom colors are lifted a little in dark mode. */
export const solid = (c: string) =>
  isHex(c) ? `light-dark(${c}, oklch(from ${c} max(l, 0.62) c h))` : `var(--c-${c})`;
/** The hue at a lightness that reads as text on a tint of itself. */
export const ink = (c: string) =>
  isHex(c) ? `light-dark(oklch(from ${c} 0.44 min(c, 0.15) h), oklch(from ${c} 0.86 min(c, 0.1) h))` : `var(--c-${c}-text)`;
/** A soft tint of the hue on the current surface, for pills and chips. */
export const tint = (c: string, pct = 16) => `color-mix(in oklch, ${solid(c)} ${pct}%, var(--surface))`;

export const PRIORITY_COLOR: Record<string, string | null> = {
  none: null,
  low: 'blue',
  medium: 'yellow',
  high: 'orange',
  urgent: 'red',
};
