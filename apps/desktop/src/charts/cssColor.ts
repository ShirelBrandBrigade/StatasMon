/**
 * Token → concrete color for the canvas. uPlot paints on a 2D context, which
 * cannot read CSS custom properties, so a series color arrives as
 * `var(--cyan)` and is resolved ONCE at mount through getComputedStyle.
 * No color literal lives in this folder; tokens.css stays the only source.
 */

const VAR_RE = /^var\(\s*(--[\w-]+)\s*(?:,\s*([^)]*))?\)$/;

/** Reads a custom property (`--cyan`) from the cascade at `el`; "" when undefined. */
export function resolveToken(name: string, el: Element = document.documentElement): string {
  return getComputedStyle(el).getPropertyValue(name).trim();
}

/**
 * Resolves `var(--x)`, `var(--x, fallback)` or a bare `--x` to the token's
 * current value. Any other string passes through unchanged.
 */
export function resolveCssColor(value: string, el?: Element): string {
  const v = value.trim();
  const m = VAR_RE.exec(v);
  const name = m ? m[1] : v.startsWith("--") ? v : undefined;
  if (name === undefined) return v;
  const resolved = resolveToken(name, el);
  return resolved || (m?.[2]?.trim() ?? "");
}

export interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

/** Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` and `rgba()` (comma or space syntax). */
export function parseColor(color: string): Rgba | undefined {
  const c = color.trim();
  if (c.startsWith("#")) {
    const hex = c.slice(1);
    if (!/^[\da-f]+$/i.test(hex)) return undefined;
    if (hex.length === 3 || hex.length === 4) {
      const [r, g, b, a] = hex.split("").map((h) => Number.parseInt(h + h, 16));
      return { r, g, b, a: hex.length === 4 ? a / 255 : 1 };
    }
    if (hex.length === 6 || hex.length === 8) {
      const n = (i: number) => Number.parseInt(hex.slice(i, i + 2), 16);
      return { r: n(0), g: n(2), b: n(4), a: hex.length === 8 ? n(6) / 255 : 1 };
    }
    return undefined;
  }
  const m = /^rgba?\(\s*([^)]+)\)$/i.exec(c);
  if (!m) return undefined;
  const parts = m[1]
    .split(/[\s,/]+/)
    .filter(Boolean)
    .map(Number);
  if (parts.length < 3 || parts.some((p) => Number.isNaN(p))) return undefined;
  return { r: parts[0], g: parts[1], b: parts[2], a: parts.length > 3 ? parts[3] : 1 };
}

/** `color` with its alpha replaced by `alpha` (0–1). Unparseable colors pass through. */
export function withAlpha(color: string, alpha: number): string {
  const p = parseColor(color);
  if (!p) return color;
  return `rgba(${p.r}, ${p.g}, ${p.b}, ${alpha})`;
}
