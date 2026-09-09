import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { LevelPill, levelMeta, Pill } from "./Pill";

test("Pill uses the tone tint and text color from tokens", () => {
  render(<Pill tone="warn">Fair · 11 min</Pill>);
  const pill = screen.getByText("Fair · 11 min");
  expect(pill).toHaveAttribute("data-tone", "warn");
  expect(pill.style.background).toBe("var(--amber-tint)");
  expect(pill.style.color).toBe("var(--amber)");
});

test("LevelPill maps every log level to a tone and a label", () => {
  render(<LevelPill level="fault" />);
  const pill = screen.getByText("Fault");
  expect(pill).toHaveAttribute("data-tone", "bad");
  expect(pill).toHaveAttribute("data-level", "fault");
  expect(levelMeta("notice")).toEqual({ tone: "note", label: "Notice" });
  expect(levelMeta("debug").tone).toBe("neutral");
  expect(levelMeta("error").tone).toBe("warn");
});
