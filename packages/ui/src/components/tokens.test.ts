import { expect, test } from "vitest";
import { clamp, cx, seriesColor, toneColor, toneTint } from "./tokens";

test("series and tones resolve to CSS variables, never literal colors", () => {
  for (const s of ["cyan", "cyan-2", "vio", "mint", "amber", "red", "other"] as const) {
    expect(seriesColor(s)).toMatch(/^var\(--[a-z0-9-]+\)$/);
  }
  for (const t of ["ok", "warn", "bad", "info", "note", "neutral"] as const) {
    expect(toneColor(t)).toMatch(/^var\(--[a-z0-9-]+\)$/);
    expect(toneTint(t)).toMatch(/^var\(--[a-z0-9-]+-tint\)$/);
  }
});

test("cx joins and drops falsy, clamp bounds", () => {
  expect(cx("a", false, undefined, "b", null)).toBe("a b");
  expect(clamp(150, 0, 100)).toBe(100);
  expect(clamp(-5, 0, 100)).toBe(0);
});
