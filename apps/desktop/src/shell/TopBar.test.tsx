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

test("shows the device name, model · link, the pills, and changes the range", () => {
  render(<TopBar deviceId="fake:iphone" />);
  // The synthetic iPhone is named after its model.
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("iPhone 16 Pro");
  expect(screen.getByText("iPhone 16 Pro · Fake")).toBeInTheDocument();
  expect(screen.getByText("Synthetic")).toHaveAttribute("data-tone", "info");
  expect(screen.queryByTestId("headline-pill")).not.toBeInTheDocument();

  expect(screen.getByRole("radio", { name: "5 min" })).toBeChecked();
  fireEvent.click(screen.getByRole("radio", { name: "15 min" }));
  expect(useAppStore.getState().range).toBe("15m");
  expect(screen.getByRole("radio", { name: "15 min" })).toBeChecked();
});

test("keeps the device name before the first headline", () => {
  act(() => {
    useAppStore.getState().patchDevice("fake:ipad", { headline: undefined });
  });
  render(<TopBar deviceId="fake:ipad" />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Studio iPad");
  expect(screen.queryByTestId("headline-pill")).not.toBeInTheDocument();
});

test("a non-nominal headline becomes one short pill with the sentence as tooltip", () => {
  act(() => {
    useAppStore.getState().patchDevice("fake:ipad", {
      headline: {
        sentence: "iPad (10th generation): FieldKit crashed",
        cause: "FieldKit crashed 2 min ago.",
        severity: "warning",
        pills: [],
        rule: "app_crashed",
      },
    });
  });
  render(<TopBar deviceId="fake:ipad" />);
  const pill = screen.getByTestId("headline-pill");
  expect(pill).toHaveTextContent("App crashed");
  expect(pill).toHaveAttribute("data-tone", "warn");
  expect(pill.closest("li")).toHaveAttribute(
    "title",
    "iPad (10th generation): FieldKit crashed. FieldKit crashed 2 min ago.",
  );
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Studio iPad");
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
