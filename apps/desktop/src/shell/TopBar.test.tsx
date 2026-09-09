import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { dispatch } from "../bridge/dispatch";
import { synthesize } from "../bridge/synthetic";
import { useExportStore } from "../export/exportStore";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { resetExportNote } from "./exportNote";
import { TopBar } from "./TopBar";

beforeEach(() => {
  useAppStore.getState().reset();
  useAppStore.getState().setRange("5m");
  telemetry.reset();
  resetExportNote();
  act(() => {
    for (const { message } of synthesize(1000)) dispatch(message);
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

test("shows the headline, cause and pills and changes the range", () => {
  render(<TopBar deviceId="fake:iphone" />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("iPhone 16 Pro looks normal");
  expect(screen.getByText("Synthetic session (no fixture found).")).toBeInTheDocument();
  expect(screen.getByText("Synthetic")).toHaveAttribute("data-tone", "info");

  expect(screen.getByRole("radio", { name: "5 min" })).toBeChecked();
  fireEvent.click(screen.getByRole("radio", { name: "15 min" }));
  expect(useAppStore.getState().range).toBe("15m");
  expect(screen.getByRole("radio", { name: "15 min" })).toBeChecked();
});

test("falls back to a waiting sentence before the first headline", () => {
  act(() => {
    useAppStore.getState().patchDevice("fake:ipad", { headline: undefined });
  });
  render(<TopBar deviceId="fake:ipad" />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
    "Waiting for iPad (10th generation)…",
  );
});

test("Export opens the Export dialog for this device", () => {
  render(<TopBar deviceId="fake:iphone" />);
  expect(useExportStore.getState().open).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  expect(useExportStore.getState().open).toBe(true);
  expect(useExportStore.getState().preset).toEqual({ deviceId: "fake:iphone" });
  resetExportNote();
  expect(useExportStore.getState().open).toBe(false);
});
