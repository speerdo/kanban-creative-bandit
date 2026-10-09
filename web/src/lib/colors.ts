// Mirrors server/src/validate.rs COLORS; the CSS tokens live in app.css.
export const COLORS = [
  'slate', 'red', 'orange', 'amber', 'yellow', 'lime', 'green', 'teal', 'cyan', 'blue', 'violet', 'pink',
] as const;
export type Color = (typeof COLORS)[number];

/** Solid hue (dots, bars, stripes). */
export const solid = (c: string) => `var(--c-${c})`;
/** The hue at a lightness that reads as text on a tint of itself. */
export const ink = (c: string) => `var(--c-${c}-text)`;
/** A soft tint of the hue on the current surface, for pills and chips. */
export const tint = (c: string, pct = 16) => `color-mix(in oklch, var(--c-${c}) ${pct}%, var(--surface))`;

export const PRIORITY_COLOR: Record<string, string | null> = {
  none: null,
  low: 'blue',
  medium: 'yellow',
  high: 'orange',
  urgent: 'red',
};
