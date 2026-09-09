import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bridge/commands";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import {
  AddDeviceDialog,
  describeFailure,
  describeOutcome,
  NO_RESPONSE_TEXT,
  PENDING_ANDROID_ID,
  validateCode,
  validatePort,
} from "./AddDeviceDialog";
import { resetDetached } from "./detached";
import { Sidebar } from "./Sidebar";

vi.mock("../bridge/commands", () => ({
  commands: {
    deviceAction: vi.fn(async () => ({ outcome: "done" })),
    pauseSession: vi.fn(async () => null),
    resumeSession: vi.fn(async () => null),
    startSession: vi.fn(async () => null),
    mark: vi.fn(async () => null),
    fakeDetach: vi.fn(async () => undefined),
    fakeReattach: vi.fn(async () => undefined),
  },
}));

const deviceAction = vi.mocked(commands.deviceAction);

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
  resetDetached();
  vi.clearAllMocks();
  deviceAction.mockResolvedValue({ outcome: "done" });
});

afterEach(cleanup);

async function flush() {
  await act(async () => {
    await Promise.resolve();
  });
}

test("opens from the sidebar, focuses the first field, and Escape closes it", () => {
  render(<Sidebar view="overview" />);
  expect(screen.queryByRole("dialog")).toBeNull();
  const opener = screen.getByRole("button", { name: "Add a device" });
  expect(opener).toHaveAttribute("aria-haspopup", "dialog");
  fireEvent.click(opener);
  const dialog = screen.getByRole("dialog", { name: "Add a device" });
  expect(dialog).toHaveAttribute("aria-modal", "true");
  const host = within(dialog).getAllByLabelText("Host")[0];
  expect(host).toHaveFocus();
  expect(host).toHaveClass("mono");
  expect(within(dialog).getByText("Pair over Wi-Fi (Android 11+)")).toBeInTheDocument();
  expect(within(dialog).getByText("USB")).toBeInTheDocument();
  expect(
    within(dialog).getByText("iPhone and iPad connect over USB automatically (Phase 3)."),
  ).toBeInTheDocument();
  expect(within(dialog).getByLabelText("Port")).toHaveValue("5555");

  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(opener).toHaveAttribute("aria-expanded", "false");
});

test("validates the pairing code and the port before calling the core", () => {
  render(<AddDeviceDialog open onClose={() => {}} />);
  const dialog = screen.getByRole("dialog");
  const pairForm = within(dialog).getByRole("form", { name: "Pair over Wi-Fi (Android 11+)" });
  const f = within(pairForm);
  fireEvent.change(f.getByLabelText("Host"), { target: { value: "192.168.1.20" } });
  fireEvent.change(f.getByLabelText("Pairing port"), { target: { value: "70000" } });
  fireEvent.change(f.getByLabelText("Pairing code"), { target: { value: "12345" } });
  fireEvent.click(f.getByRole("button", { name: "Pair" }));
  expect(deviceAction).not.toHaveBeenCalled();
  expect(f.getByText("Port must be a number from 1 to 65535.")).toBeInTheDocument();
  expect(f.getByText("The pairing code is 6 digits.")).toBeInTheDocument();
  expect(f.getByLabelText("Pairing code")).toHaveAttribute("aria-invalid", "true");
  expect(f.getByLabelText("Host")).not.toHaveAttribute("aria-invalid");

  expect(validateCode("123456")).toBeUndefined();
  expect(validateCode("12345a")).toBeDefined();
  expect(validatePort("1")).toBeUndefined();
  expect(validatePort("65535")).toBeUndefined();
  expect(validatePort("0")).toBeDefined();
  expect(validatePort("65536")).toBeDefined();
  expect(validatePort("12.5")).toBeDefined();
});

test("Pair sends adb_pair with the placeholder id and shows the outcome in mint", async () => {
  render(<AddDeviceDialog open onClose={() => {}} />);
  const pairForm = screen.getByRole("form", { name: "Pair over Wi-Fi (Android 11+)" });
  const f = within(pairForm);
  fireEvent.change(f.getByLabelText("Host"), { target: { value: " 192.168.1.20 " } });
  fireEvent.change(f.getByLabelText("Pairing port"), { target: { value: "37001" } });
  fireEvent.change(f.getByLabelText("Pairing code"), { target: { value: "123456" } });
  fireEvent.click(f.getByRole("button", { name: "Pair" }));
  expect(deviceAction).toHaveBeenCalledWith(PENDING_ANDROID_ID, {
    action: "adb_pair",
    host: "192.168.1.20",
    port: 37001,
    code: "123456",
  });
  expect(PENDING_ANDROID_ID).toBe("android:pending");
  await flush();
  const out = f.getByRole("status");
  expect(out).toHaveAttribute("data-outcome", "ok");
  expect(out).toHaveClass("text-mint");
  expect(out).toHaveTextContent("Paired. Now connect to the device below.");
  // The host is carried over to the Connect form.
  const connForm = screen.getByRole("form", { name: "Connect" });
  expect(within(connForm).getByLabelText("Host")).toHaveValue("192.168.1.20");
});

test("Connect sends adb_connect; a rejected command keeps the dialog open and shows the hint in red", async () => {
  const onClose = vi.fn();
  deviceAction.mockRejectedValueOnce({
    code: "adb_not_found",
    message: "adb not found",
    hint: "Install platform-tools.",
  });
  render(<AddDeviceDialog open onClose={onClose} />);
  const connForm = screen.getByRole("form", { name: "Connect" });
  const f = within(connForm);
  fireEvent.change(f.getByLabelText("Host"), { target: { value: "10.0.0.5" } });
  fireEvent.click(f.getByRole("button", { name: "Connect" }));
  expect(deviceAction).toHaveBeenCalledWith(PENDING_ANDROID_ID, {
    action: "adb_connect",
    host: "10.0.0.5",
    port: 5555,
  });
  await flush();
  const out = f.getByRole("status");
  expect(out).toHaveAttribute("data-outcome", "fail");
  expect(out).toHaveClass("text-red");
  expect(out).toHaveTextContent("adb not found");
  expect(out).toHaveTextContent("Install platform-tools.");
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog")).toBeInTheDocument();
});

test("a null answer (browser replay) shows a neutral No response line", async () => {
  deviceAction.mockResolvedValueOnce(null);
  render(<AddDeviceDialog open onClose={() => {}} />);
  const f = within(screen.getByRole("form", { name: "Connect" }));
  fireEvent.change(f.getByLabelText("Host"), { target: { value: "10.0.0.5" } });
  fireEvent.click(f.getByRole("button", { name: "Connect" }));
  await flush();
  const out = f.getByRole("status");
  expect(out).toHaveAttribute("data-outcome", "none");
  expect(out).toHaveTextContent(NO_RESPONSE_TEXT);
  expect(out).toHaveClass("text-mut");
});

test("the Developer options help expands, and Tab wraps inside the dialog", () => {
  render(<AddDeviceDialog open onClose={() => {}} />);
  const dialog = screen.getByRole("dialog");
  const help = within(dialog).getByRole("button", { name: "Where is Developer options?" });
  expect(help).toHaveAttribute("aria-expanded", "false");
  fireEvent.click(help);
  expect(help).toHaveAttribute("aria-expanded", "true");
  expect(within(dialog).getByText(/tap Build number seven times/)).toBeInTheDocument();

  // Shift+Tab from the first focusable wraps to the last one.
  const close = within(dialog).getByRole("button", { name: "Close" });
  close.focus();
  fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
  expect(help).toHaveFocus();
  fireEvent.keyDown(dialog, { key: "Tab" });
  expect(close).toHaveFocus();
});

test("describeOutcome / describeFailure cover the core's shapes", () => {
  expect(describeOutcome(undefined, "pair").kind).toBe("none");
  expect(describeOutcome("connected to 10.0.0.5:5555", "connect")).toEqual({
    kind: "ok",
    text: "connected to 10.0.0.5:5555",
  });
  expect(describeOutcome({ outcome: "started", task_id: "t1" }, "connect").text).toBe(
    "Connecting…",
  );
  expect(describeOutcome({ outcome: "file", path: "/x" }, "pair").text).toBe("/x");
  expect(describeOutcome({ ok: false, message: "failed to authenticate" }, "pair")).toEqual({
    kind: "fail",
    text: "failed to authenticate",
    hint: undefined,
  });
  expect(describeFailure(new Error("boom"))).toEqual({
    kind: "fail",
    text: "boom",
    hint: undefined,
  });
  expect(describeFailure("plain")).toEqual({ kind: "fail", text: "plain" });
});
