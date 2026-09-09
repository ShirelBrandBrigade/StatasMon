import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { commands } from "./bridge/commands";
import { dispatch } from "./bridge/dispatch";
import { synthesize } from "./bridge/synthetic";
import { useAppStore } from "./store/app";
import { telemetry } from "./store/telemetry";

vi.mock("./bridge", () => ({
  startBridge: vi.fn(async () => () => {}),
  getStream: vi.fn(async () => ({ kind: "replay", invoke: async () => null })),
}));

vi.mock("./bridge/commands", () => ({
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
  window.location.hash = "";
  useAppStore.getState().reset();
  telemetry.reset();
  vi.clearAllMocks();
});

test("shows the empty state when no device is attached", () => {
  render(<App />);
  expect(screen.getByText("StatasMon")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "No device connected" })).toBeInTheDocument();
});

test("routes to the Overview by default and to placeholder pages by hash", () => {
  act(() => {
    for (const { message } of synthesize(1000)) dispatch(message);
  });
  const { container } = render(<App />);
  expect(container.querySelectorAll("[data-slot]")).toHaveLength(8);
  act(() => {
    window.location.hash = "#logs";
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  });
  expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument();
  expect(screen.getByText("Coming in Phase 6")).toBeInTheDocument();
});

test("keeps the #gallery route", () => {
  window.location.hash = "#gallery";
  render(<App />);
  expect(screen.queryByText("No device connected")).toBeNull();
  expect(screen.getByText("Segment")).toBeInTheDocument();
});

test("M marks the active session unless typing in a field", () => {
  act(() => {
    for (const { message } of synthesize(1000)) dispatch(message);
  });
  render(<App />);
  fireEvent.keyDown(window, { key: "m" });
  expect(commands.mark).toHaveBeenCalledWith("fake:iphone", "Mark 1");
  const input = document.createElement("input");
  document.body.appendChild(input);
  fireEvent.keyDown(input, { key: "m" });
  expect(commands.mark).toHaveBeenCalledTimes(1);
  input.remove();
});
