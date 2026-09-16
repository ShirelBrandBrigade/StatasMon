import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { CoreBars } from "./CoreBars";

const cores = [
  { index: 0, kind: "performance" as const, usagePct: 62, online: true },
  { index: 1, kind: "performance" as const, usagePct: 58, online: true },
  { index: 2, kind: "efficiency" as const, usagePct: 31, online: true },
  { index: 3, kind: "efficiency" as const, usagePct: null, online: false },
];

test("CoreBars labels P/E cores, colors them and reports offline cores", () => {
  render(<CoreBars cores={cores} />);
  const p0 = screen.getByRole("img", { name: "P0 62%" });
  expect(p0.querySelector("b")?.style.height).toBe("62%");
  expect(p0.querySelector("b")?.style.background).toBe("var(--cyan)");
  const e0 = screen.getByRole("img", { name: "E0 31%" });
  expect(e0.querySelector("b")?.style.background).toBe("var(--cyan-2)");
  expect(screen.getByRole("img", { name: "E1 offline" })).toHaveClass("is-offline");
  // One label per bar, in core order, so each sits under its own bar.
  expect(
    Array.from(document.querySelectorAll(".sm-cores-labels > span")).map((s) => s.textContent),
  ).toEqual(["E0", "E1", "P0", "P1"]);
});
