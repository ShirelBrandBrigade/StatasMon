import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { dispatch } from "../bridge/dispatch";
import { NOTICE_LIMIT, useAppStore } from "../store/app";
import { EmptyState } from "./EmptyState";
import { NOTICE_AUTO_DISMISS_MS, Notices } from "./Notices";

beforeEach(() => {
  vi.useFakeTimers();
  useAppStore.getState().reset();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

test("renders a core error as a notice with its code, message and hint; Dismiss removes it", () => {
  vi.spyOn(console, "warn").mockImplementation(() => {});
  render(<Notices />);
  const list = screen.getByRole("list", { name: "Notices" });
  expect(list).toHaveAttribute("data-count", "0");
  act(() => {
    dispatch({
      type: "error",
      device_id: null,
      code: "adb_not_found",
      message: "adb not found",
      hint: "Install platform-tools and restart StatasMon.",
    });
  });
  expect(list).toHaveAttribute("data-count", "1");
  const item = within(list).getByRole("status");
  expect(item).toHaveTextContent("adb not found");
  expect(item).toHaveTextContent("Install platform-tools and restart StatasMon.");
  const pill = item.querySelector(".sm-pill");
  expect(pill).toHaveAttribute("data-tone", "bad");
  expect(pill).toHaveTextContent("adb not found");

  // Persistent: still there long after the auto-dismiss window.
  act(() => {
    vi.advanceTimersByTime(NOTICE_AUTO_DISMISS_MS * 3);
  });
  expect(list).toHaveAttribute("data-count", "1");

  fireEvent.click(within(item).getByRole("button", { name: "Dismiss" }));
  expect(list).toHaveAttribute("data-count", "0");
  expect(useAppStore.getState().notices).toEqual([]);
});

test("warnings auto-dismiss after 12 s and use the warn tone", () => {
  render(<Notices />);
  act(() => {
    useAppStore.getState().pushNotice({ code: "io", message: "shell closed", deviceId: "a:1" });
  });
  const list = screen.getByRole("list", { name: "Notices" });
  const pill = within(list).getByRole("status").querySelector(".sm-pill");
  expect(pill).toHaveAttribute("data-tone", "warn");
  expect(pill).toHaveTextContent("io");
  act(() => {
    vi.advanceTimersByTime(NOTICE_AUTO_DISMISS_MS - 1);
  });
  expect(list).toHaveAttribute("data-count", "1");
  act(() => {
    vi.advanceTimersByTime(1);
  });
  expect(list).toHaveAttribute("data-count", "0");
});

test("the store keeps only the last 20 notices", () => {
  const s = useAppStore.getState();
  for (let i = 0; i < NOTICE_LIMIT + 5; i += 1) s.pushNotice({ code: "parse", message: `n${i}` });
  const notices = useAppStore.getState().notices;
  expect(notices).toHaveLength(NOTICE_LIMIT);
  expect(notices[0].message).toBe("n5");
  expect(notices[notices.length - 1].message).toBe(`n${NOTICE_LIMIT + 4}`);
});

test("EmptyState repeats persistent notices with their hint, not transient ones", () => {
  const s = useAppStore.getState();
  s.pushNotice({
    code: "driver_missing",
    message: "No USB driver",
    hint: "Install the OEM driver.",
  });
  s.pushNotice({ code: "io", message: "transient" });
  render(<EmptyState />);
  const block = screen.getByRole("list", { name: "Blocking problems" });
  expect(block).toHaveTextContent("No USB driver");
  expect(block).toHaveTextContent("Install the OEM driver.");
  expect(block).not.toHaveTextContent("transient");
});
