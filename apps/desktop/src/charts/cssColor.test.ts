import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeAll, expect, test } from "vitest";
import { parseColor, resolveCssColor, resolveToken, withAlpha } from "./cssColor";
import { applyTokens } from "./testing/jsdomCanvas";

// Vitest defines `__dirname`; `new URL(…, import.meta.url)` is rewritten by Vite and `?raw` CSS imports come back empty.
const TOKENS_CSS = resolve(__dirname, "../../../../packages/ui/src/tokens.css");
const tokens: Record<string, string> = {};

beforeAll(() => {
  const css = readFileSync(TOKENS_CSS, "utf8");
  for (const m of css.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    tokens[m[1]] = m[2].replace(/\s+/g, " ").trim();
  }
  applyTokens(css);
});

test("resolves var() references and bare token names from the cascade", () => {
  expect(resolveToken("--cyan")).toBe(tokens["--cyan"]);
  expect(resolveCssColor("var(--cyan)")).toBe(tokens["--cyan"]);
  expect(resolveCssColor(" var( --vio ) ")).toBe(tokens["--vio"]);
  expect(resolveCssColor("--grid")).toBe(tokens["--grid"]);
  expect(resolveCssColor("var(--nope, currentColor)")).toBe("currentColor");
  expect(resolveCssColor("var(--nope)")).toBe("");
  expect(resolveCssColor("currentColor")).toBe("currentColor");
});

test("withAlpha rewrites hex and rgba tokens to rgba() with the requested alpha", () => {
  const cyan = parseColor(tokens["--cyan"]);
  expect(cyan).toBeDefined();
  expect(withAlpha(tokens["--cyan"], 0.35)).toBe(`rgba(${cyan?.r}, ${cyan?.g}, ${cyan?.b}, 0.35)`);
  expect(withAlpha(tokens["--cyan"], 0)).toBe(`rgba(${cyan?.r}, ${cyan?.g}, ${cyan?.b}, 0)`);
  // rgba token: alpha is replaced, channels kept.
  const grid = parseColor(tokens["--grid"]);
  expect(grid?.a).toBeCloseTo(0.06);
  expect(withAlpha(tokens["--grid"], 0.5)).toBe(`rgba(${grid?.r}, ${grid?.g}, ${grid?.b}, 0.5)`);
  // The cyan tint token is the cyan channels at .16 (design rule), which parseColor must agree on.
  const tint = parseColor(tokens["--cyan-tint"]);
  expect([tint?.r, tint?.g, tint?.b]).toEqual([cyan?.r, cyan?.g, cyan?.b]);
  expect(tint?.a).toBeCloseTo(0.16);
});

test("parseColor handles short hex, hex alpha, space syntax and rejects junk", () => {
  expect(parseColor("#fff")).toEqual({ r: 255, g: 255, b: 255, a: 1 });
  expect(parseColor("#0008")?.a).toBeCloseTo(136 / 255);
  expect(parseColor("#00000080")?.a).toBeCloseTo(128 / 255);
  expect(parseColor("rgb(1 2 3 / 0.4)")).toEqual({ r: 1, g: 2, b: 3, a: 0.4 });
  expect(parseColor("rgb(1, 2, 3)")).toEqual({ r: 1, g: 2, b: 3, a: 1 });
  expect(parseColor("hsl(1 2% 3%)")).toBeUndefined();
  expect(parseColor("#12")).toBeUndefined();
  expect(withAlpha("currentColor", 0.5)).toBe("currentColor");
});
