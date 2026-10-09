// Applies a user's appearance prefs to the page.
//
// Prefs become attributes on <html> (data-theme, data-density) plus a handful of CSS variables
// (accent, canvas, and the computed text colors that keep both readable). The result is cached
// in localStorage and re-applied by an inline script in index.html before the app loads, so a
// reload never flashes the wrong theme.

import type { Prefs } from './api';
import { COLORS } from './colors';
import { DARK_TEXT, hexToRgb, mutedOn, textOn, type Rgb } from './contrast';

const CACHE_KEY = 'kanban.theme';

export type ThemeCache = { attrs: Record<string, string>; vars: Record<string, string> };

const isHex = (c: string) => c.startsWith('#');

/** CSS for a palette name or hex color, adapted to the current light/dark scheme. */
function themed(c: string): string {
  // A custom hex keeps its hue in dark mode but is lifted so it still glows on black.
  return isHex(c) ? `light-dark(${c}, oklch(from ${c} max(l, 0.7) c h))` : `var(--c-${c})`;
}

function canvasFor(c: string): string {
  if (isHex(c)) return c;
  // Palette backgrounds are soft washes of the hue over the brand paper / base.
  return `light-dark(color-mix(in oklch, var(--c-${c}) 22%, var(--brand-paper)), color-mix(in oklch, var(--c-${c}) 24%, var(--brand-base)))`;
}

function canvasImage(style: Prefs['background_style']): [string, string] {
  switch (style) {
    case 'gradient':
      return [
        'linear-gradient(160deg, transparent 0%, color-mix(in oklch, var(--accent) 22%, transparent) 100%)',
        'auto',
      ];
    case 'subtle-pattern':
      return ['radial-gradient(color-mix(in oklch, var(--on-canvas) 14%, transparent) 1px, transparent 1.4px)', '18px 18px'];
    default:
      return ['none', 'auto'];
  }
}

// ---- resolving real colors -----------------------------------------------------------------

let probe: HTMLElement | null = null;
let ctx: CanvasRenderingContext2D | null = null;

/** The sRGB value of any CSS color expression, as the page currently renders it. */
export function resolve(css: string): Rgb {
  if (!probe) {
    probe = document.createElement('span');
    probe.style.cssText = 'position:absolute;width:0;height:0;overflow:hidden;pointer-events:none';
    document.body.appendChild(probe);
    ctx = document.createElement('canvas').getContext('2d', { willReadFrequently: true });
  }
  probe.style.color = '';
  probe.style.color = css;
  const computed = getComputedStyle(probe).color; // may be rgb(), oklch() or color(srgb …)
  if (!ctx) return [128, 128, 128];
  // Let the canvas convert whatever notation the browser returned into sRGB bytes.
  ctx.clearRect(0, 0, 1, 1);
  ctx.fillStyle = '#808080';
  ctx.fillStyle = computed;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
  return [r, g, b];
}

/** Readable text color for a palette name or hex color (avatars, solid chips). */
export function onColor(c: string): string {
  if (!isHex(c)) return `var(--c-${c}-on, #fff)`;
  // In dark mode `solid()` lifts custom colors to OKLCH lightness ≥ 0.62, where dark text
  // always reads best; in light mode the color is used as is.
  return `light-dark(${textOn(hexToRgb(c))}, ${DARK_TEXT})`;
}

// ---- applying ------------------------------------------------------------------------------

/** Applies prefs to the page now, and caches the result for the next load. */
export function applyPrefs(prefs: Prefs) {
  const root = document.documentElement;
  const attrs: Record<string, string> = { density: prefs.density };
  if (prefs.theme !== 'system') attrs.theme = prefs.theme;

  // Attributes first: the computed colors below depend on the resulting scheme.
  delete root.dataset.theme;
  for (const [k, v] of Object.entries(attrs)) root.dataset[k] = v;

  const vars: Record<string, string> = {};
  const set = (k: string, v: string) => {
    vars[k] = v;
    root.style.setProperty(k, v);
  };
  for (const k of [...root.style]) if (k.startsWith('--')) root.style.removeProperty(k);

  set('--accent', themed(prefs.accent_color));
  set('--accent-text', textOn(resolve('var(--accent)')));

  if (prefs.background_color) {
    set('--canvas', canvasFor(prefs.background_color));
    const canvas = resolve('var(--canvas)');
    set('--on-canvas', textOn(canvas));
    set('--on-canvas-muted', mutedOn(canvas));
  }
  const [image, size] = canvasImage(prefs.background_style);
  if (image !== 'none') {
    set('--canvas-image', image);
    set('--canvas-size', size);
  }

  // Text on each solid palette hue (e.g. avatar initials), for the scheme in effect.
  for (const c of COLORS) set(`--c-${c}-on`, textOn(resolve(`var(--c-${c})`)));

  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ attrs, vars } satisfies ThemeCache));
  } catch {
    /* fine: the next load just themes a moment later */
  }
}

/** For the system theme, computed colors depend on the OS setting, so redo them on change. */
export function watchSystemTheme(current: () => Prefs | undefined) {
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    const p = current();
    if (p && p.theme === 'system') applyPrefs(p);
  });
}

export function clearThemeCache() {
  try {
    localStorage.removeItem(CACHE_KEY);
  } catch {
    /* ignore */
  }
}
