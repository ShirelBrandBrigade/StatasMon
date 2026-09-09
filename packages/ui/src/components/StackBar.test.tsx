import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { StackBar } from "./StackBar";

test("StackBar sizes segments against the total and paints them by series", () => {
  const { container } = render(
    <StackBar
      name="Memory breakdown"
      total={8}
      segments={[
        { label: "App", value: 3.1, series: "cyan" },
        { label: "Wired", value: 1.4, series: "vio" },
      ]}
    />,
  );
  expect(screen.getByRole("img", { name: "Memory breakdown" })).toBeInTheDocument();
  const segs = container.querySelectorAll("i");
  expect(segs).toHaveLength(2);
  expect(segs[0].style.width).toBe("38.75%");
  expect(segs[0].style.background).toBe("var(--cyan)");
  expect(segs[1].style.width).toBe("17.5%");
});

test("StackBar normalises to the sum when no total is given", () => {
  const { container } = render(
    <StackBar
      name="x"
      segments={[
        { label: "a", value: 1, series: "mint" },
        { label: "b", value: 3, series: "other" },
      ]}
    />,
  );
  expect(container.querySelector("i")?.style.width).toBe("25%");
});
