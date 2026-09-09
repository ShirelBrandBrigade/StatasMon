import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { dispatch } from "../bridge/dispatch";
import { synthesize } from "../bridge/synthetic";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { Overview, SLOT_NAMES } from "./Overview";

afterEach(cleanup);

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
  act(() => {
    for (const { message } of synthesize(1000)) dispatch(message);
  });
});

test("renders the headline and every card slot as a placeholder", () => {
  const { container } = render(<Overview />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("iPhone 16 Pro looks normal");
  const slots = [...container.querySelectorAll("[data-slot]")].map((el) =>
    el.getAttribute("data-slot"),
  );
  expect(slots).toEqual([...SLOT_NAMES]);
  expect(container.querySelectorAll("[data-slot].glass")).toHaveLength(SLOT_NAMES.length);
  expect(screen.getByText("Top apps")).toBeInTheDocument();
});

test("a provided slot component replaces the placeholder and receives the active device id", () => {
  const Cpu = ({ deviceId }: { deviceId: string }) => (
    <section className="glass" data-testid="cpu-card">
      cpu for {deviceId}
    </section>
  );
  const { container } = render(<Overview slots={{ cpu: Cpu }} />);
  expect(screen.getByTestId("cpu-card")).toHaveTextContent("cpu for fake:iphone");
  expect(container.querySelector('[data-slot="cpu"]')).toContainElement(
    screen.getByTestId("cpu-card"),
  );
  expect(container.querySelectorAll("[data-slot]")).toHaveLength(SLOT_NAMES.length);
});

test("renders nothing without an active device", () => {
  act(() => useAppStore.getState().reset());
  const { container } = render(<Overview />);
  expect(container).toBeEmptyDOMElement();
});
