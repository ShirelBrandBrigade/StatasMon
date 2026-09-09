import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { dispatch } from "../bridge/dispatch";
import { synthesize } from "../bridge/synthetic";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { resetDetached } from "./detached";
import { Sidebar } from "./Sidebar";

vi.mock("../bridge/commands", () => ({
  commands: {
    pauseSession: vi.fn(async () => null),
    resumeSession: vi.fn(async () => null),
    startSession: vi.fn(async () => null),
    mark: vi.fn(async () => null),
    fakeDetach: vi.fn(async () => undefined),
    fakeReattach: vi.fn(async () => undefined),
  },
}));

afterEach(cleanup);

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
  resetDetached();
  act(() => {
    for (const { message } of synthesize(2000)) dispatch(message);
  });
});

test("lists every device with its transport · state line and switches the active one on click", () => {
  render(<Sidebar view="overview" />);
  const list = within(screen.getByRole("list", { name: "Devices" }));
  const cards = list.getAllByRole("button").filter((b) => b.hasAttribute("aria-pressed"));
  expect(cards.map((c) => c.getAttribute("data-device-id"))).toEqual([
    "fake:iphone",
    "fake:ipad",
    "fake:android",
  ]);
  expect(list.getByText("iPhone 16 Pro")).toBeInTheDocument();
  expect(list.getByText("Studio iPad")).toBeInTheDocument();
  expect(list.getByText("Galaxy Tab S8")).toBeInTheDocument();
  expect(list.getAllByText("USB · recording")).toHaveLength(2);
  expect(list.getByText("Wi-Fi ADB · recording")).toBeInTheDocument();
  expect(cards[0]).toHaveAttribute("aria-pressed", "true");

  fireEvent.click(cards[1]);
  expect(useAppStore.getState().activeDeviceId).toBe("fake:ipad");
  expect(cards[1]).toHaveAttribute("aria-pressed", "true");
  expect(cards[0]).toHaveAttribute("aria-pressed", "false");
});

test("nav lists every view, marks the current one and links by hash", () => {
  render(<Sidebar view="network" />);
  const nav = within(screen.getByRole("navigation", { name: "Views" }));
  expect(nav.getAllByRole("link")).toHaveLength(8);
  const current = nav.getByRole("link", { name: "Network" });
  expect(current).toHaveAttribute("aria-current", "page");
  expect(current).toHaveAttribute("href", "#network");
  expect(nav.getByRole("link", { name: "Overview" })).not.toHaveAttribute("aria-current");
});

test("device icon follows the platform and model", () => {
  render(<Sidebar view="overview" />);
  const icons = screen
    .getAllByRole("button")
    .filter((b) => b.hasAttribute("aria-pressed"))
    .map((c) => c.querySelector("svg")?.getAttribute("data-icon"));
  expect(icons).toEqual(["phone", "tablet", "android"]);
});
