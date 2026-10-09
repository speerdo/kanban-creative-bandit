// WCAG 2.x contrast. Text on any colored surface is picked from these two, whichever reads
// better, so a bright yellow avatar gets dark text and a deep blue button gets white.

export const DARK_TEXT = '#0b0b0c';
export const LIGHT_TEXT = '#ffffff';

export type Rgb = [number, number, number];

/** Relative luminance of an sRGB color (0..255 channels). */
export function luminance([r, g, b]: Rgb): number {
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

export function hexToRgb(hex: string): Rgb {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** The better-reading text color for a background. */
export function textOn(bg: Rgb): string {
  return contrast(bg, hexToRgb(DARK_TEXT)) >= contrast(bg, hexToRgb(LIGHT_TEXT)) ? DARK_TEXT : LIGHT_TEXT;
}

/** Muted text on a background: the text color pulled toward the background, still ≥ 4.5:1. */
export function mutedOn(bg: Rgb): string {
  const text = hexToRgb(textOn(bg));
  for (let t = 0.45; t > 0; t -= 0.05) {
    const mixed = text.map((c, i) => Math.round(c + (bg[i] - c) * t)) as Rgb;
    if (contrast(mixed, bg) >= 4.5) return `rgb(${mixed.join(' ')})`;
  }
  return `rgb(${text.join(' ')})`;
}
