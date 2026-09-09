import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { Legend } from "./Legend";

test("Legend renders swatch, label and mono value", () => {
  render(
    <Legend
      items={[
        { label: "App", value: "3.1 GB", series: "cyan" },
        { label: "Wired", value: "1.4 GB", series: "vio" },
      ]}
    />,
  );
  const value = screen.getByText("3.1 GB");
  expect(value.tagName).toBe("EM");
  expect(value).toHaveClass("mono");
  const swatch = value.parentElement?.querySelector("i");
  expect(swatch?.style.background).toBe("var(--cyan)");
  expect(screen.getByText("1.4 GB")).toBeInTheDocument();
});
