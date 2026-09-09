import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { Card, CardTitle } from "./Card";

test("Card renders title, subtitle, children and footer with the glass class", () => {
  render(
    <Card title="CPU" subtitle="A18 Pro · 2P + 4E" footer={<span>footer</span>} data-testid="c">
      <p>body</p>
    </Card>,
  );
  const card = screen.getByTestId("c");
  expect(card).toHaveClass("glass", "sm-card");
  expect(screen.getByText("CPU")).toBeInTheDocument();
  expect(screen.getByText("A18 Pro · 2P + 4E")).toBeInTheDocument();
  expect(screen.getByText("body")).toBeInTheDocument();
  expect(screen.getByText("footer").parentElement).toHaveClass("sm-card-footer");
});

test("CardTitle omits the subtitle slot when none is given", () => {
  const { container } = render(<CardTitle title="Memory" />);
  expect(container.querySelectorAll("span")).toHaveLength(0);
});
