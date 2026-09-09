import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bridge/commands";
import { dispatch } from "../bridge/dispatch";
import { synthesize } from "../bridge/synthetic";
import { useAppStore } from "../store/app";
import { useLogFilterStore } from "../store/logFilter";
import { telemetry } from "../store/telemetry";
import { ExportDialog } from "./ExportDialog";
import {
  buildScope,
  defaultExportPath,
  pickExportProgress,
  resultLine,
  toProtocolFilter,
  withExtension,
} from "./exportModel";
import { EXPORT_EVENT, openExportDialog, useExportStore } from "./exportStore";

vi.mock("../bridge/commands", () => ({
  commands: {
    exportData: vi.fn(),
  },
}));

const exportData = vi.mocked(commands.exportData);

beforeEach(() => {
  useAppStore.getState().reset();
  useAppStore.getState().setRange("5m");
  useLogFilterStore.getState().reset();
  useExportStore.getState().close();
  telemetry.reset();
  vi.clearAllMocks();
  act(() => {
    for (const { message } of synthesize(2000)) dispatch(message);
    dispatch({
      type: "processes",
      device_id: "fake:iphone",
      ts_ms: Date.now(),
      rows: [
        {
          pid: 1,
          name: "FieldKit",
          app_id: "com.example.fieldkit",
          cpu_pct: 1,
          mem_bytes: null,
          threads: null,
          net_bps: null,
          state: "foreground",
          energy: null,
        },
        {
          pid: 2,
          name: "kernel_task",
          app_id: null,
          cpu_pct: 1,
          mem_bytes: null,
          threads: null,
          net_bps: null,
          state: "foreground",
          energy: null,
        },
      ],
    });
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

test("stays hidden until opened; the window event opens it with a preset", () => {
  render(<ExportDialog />);
  expect(screen.queryByRole("dialog")).toBeNull();
  act(() => {
    window.dispatchEvent(
      new CustomEvent(EXPORT_EVENT, { detail: { scope: "selection", seqs: [4, 5, 6] } }),
    );
  });
  expect(screen.getByRole("dialog", { name: "Export" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "Selected log lines (3)" })).toBeChecked();
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("shows formats and scopes, prefills the path and follows the format's extension", () => {
  render(<ExportDialog />);
  act(() => openExportDialog({ deviceId: "fake:iphone" }));
  expect(screen.getByText(/Live session of/)).toHaveTextContent("iPhone 16 Pro");
  for (const f of ["Log", "JSONL", "CSV", "Bundle"]) {
    expect(screen.getByRole("radio", { name: f })).toBeInTheDocument();
  }
  expect(screen.getByRole("radio", { name: "Whole session" })).toBeChecked();
  expect(screen.getByRole("radio", { name: "Current range (5m)" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "Current log filter" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "One app" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "Selected log lines (0)" })).toBeDisabled();

  const path = screen.getByLabelText("Destination") as HTMLInputElement;
  expect(path.value).toMatch(/^~\/Downloads\/StatasMon\/iphone-16-pro-\d{4}-\d\d-\d\d-\d{4}\.log$/);
  fireEvent.click(screen.getByRole("radio", { name: "Bundle" }));
  expect(path.value).toMatch(/\.zip$/);
  // Once edited, the path is the user's.
  fireEvent.change(path, { target: { value: "/tmp/mine.zip" } });
  fireEvent.click(screen.getByRole("radio", { name: "CSV" }));
  expect(path.value).toBe("/tmp/mine.zip");
  // Browser: no native picker, no Reveal.
  expect(screen.getByRole("button", { name: "Choose…" })).toBeDisabled();
});

test("validates the app scope and the path before calling the core", () => {
  render(<ExportDialog />);
  act(() => openExportDialog({ deviceId: "fake:iphone" }));
  fireEvent.click(screen.getByRole("radio", { name: "One app" }));
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  expect(screen.getByRole("alert")).toHaveTextContent("Pick an app to export.");
  expect(exportData).not.toHaveBeenCalled();

  const app = screen.getByRole("combobox", { name: "App" });
  expect(app).not.toBeDisabled();
  fireEvent.change(app, { target: { value: "com.example.fieldkit" } });
  fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "   " } });
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  expect(screen.getByRole("alert")).toHaveTextContent("Choose where to write the file.");
  expect(exportData).not.toHaveBeenCalled();
});

test("runs an export: progress from the stream, then the result line; errors show code + hint", async () => {
  let resolve: (r: { path: string; bytes: number; entries: number }) => void = () => {};
  exportData.mockImplementationOnce(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  render(<ExportDialog />);
  act(() => openExportDialog({ deviceId: "fake:iphone" }));
  fireEvent.click(screen.getByRole("radio", { name: "JSONL" }));
  fireEvent.click(screen.getByRole("radio", { name: "Current log filter" }));
  fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "/tmp/out.jsonl" } });
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  expect(exportData).toHaveBeenCalledTimes(1);
  const req = exportData.mock.calls[0][0];
  expect(req.device_id).toBe("fake:iphone");
  expect(req.session_id).toBeNull();
  expect(req.format).toBe("jsonl");
  expect(req.path).toBe("/tmp/out.jsonl");
  expect(req.scope.scope).toBe("filter");
  if (req.scope.scope === "filter") {
    expect(req.scope.filter.device_id).toBe("fake:iphone");
    expect(req.scope.filter.levels).toEqual(["info", "notice", "warning", "error", "fault"]);
  }

  expect(screen.getByRole("button", { name: "Exporting…" })).toBeDisabled();
  const bar = screen.getByRole("progressbar", { name: "Export progress" });
  expect(bar).toHaveValue(0);
  act(() => {
    dispatch({ type: "progress", task_id: "export-1", pct: 40, message: "export: writing" });
  });
  expect(bar).toHaveValue(40);
  expect(screen.getByTestId("export-progress")).toHaveTextContent("40 % · export: writing");
  // Unrelated progress is ignored.
  act(() => {
    dispatch({ type: "progress", task_id: "tunnel", pct: 99, message: "tunnel: up" });
  });
  expect(bar).toHaveValue(40);

  await act(async () => {
    resolve({ path: "/tmp/out.jsonl", bytes: 12_345, entries: 100 });
  });
  const result = screen.getByTestId("export-result");
  expect(result).toHaveTextContent("Wrote 12 345 B · 100 entries · /tmp/out.jsonl");
  expect(screen.getByRole("button", { name: "Reveal" })).toBeDisabled();

  exportData.mockRejectedValueOnce({
    code: "io",
    message: "Permission denied",
    hint: "Choose a folder you can write to.",
  });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
  });
  const fail = screen.getByRole("status");
  expect(fail).toHaveAttribute("data-outcome", "fail");
  expect(fail).toHaveTextContent("io · Permission denied");
  expect(fail).toHaveTextContent("Choose a folder you can write to.");
});

test("a stored-session preset exports that session", async () => {
  exportData.mockResolvedValueOnce({ path: "/tmp/s.zip", bytes: 1, entries: 1 });
  render(<ExportDialog />);
  act(() =>
    openExportDialog({
      deviceId: "fake:android",
      deviceName: "Galaxy Tab S8",
      sessionId: "stored-1",
      scope: "session",
      format: "bundle",
    }),
  );
  expect(screen.getByText(/Stored session of/)).toHaveTextContent("Galaxy Tab S8");
  expect((screen.getByLabelText("Destination") as HTMLInputElement).value).toMatch(
    /galaxy-tab-s8-.*\.zip$/,
  );
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
  });
  expect(exportData.mock.calls[0][0].session_id).toBe("stored-1");
  expect(exportData.mock.calls[0][0].format).toBe("bundle");
});

test("model helpers: scopes, filter conversion, progress correlation, formatting", () => {
  const filter = toProtocolFilter("d", null, {
    processes: ["FieldKit"],
    levels: ["error", "fault"],
    query: "/time.?out/i",
    followTail: true,
  });
  expect(filter).toMatchObject({
    device_id: "d",
    session_id: null,
    processes: ["FieldKit"],
    levels: ["error", "fault"],
    query: "time.?out",
    regex: true,
  });
  expect(
    toProtocolFilter("d", "s", { processes: [], levels: [], query: "  ", followTail: false }),
  ).toMatchObject({ session_id: "s", query: null, regex: false });

  const base = { rangeMs: 300_000, nowMs: 1_000_000, filter, appId: "", seqs: [] };
  expect(buildScope({ ...base, kind: "session" })).toEqual({ scope: { scope: "session" } });
  expect(buildScope({ ...base, kind: "range" })).toEqual({
    scope: { scope: "range", from_ms: 700_000, to_ms: 1_000_000 },
  });
  expect(buildScope({ ...base, kind: "range", rangeMs: undefined })).toEqual({
    scope: { scope: "session" },
  });
  expect(buildScope({ ...base, kind: "app" })).toEqual({ error: "Pick an app to export." });
  expect(buildScope({ ...base, kind: "app", appId: "x" })).toEqual({
    scope: { scope: "app", app_id: "x" },
  });
  expect(buildScope({ ...base, kind: "selection" })).toHaveProperty("error");
  expect(buildScope({ ...base, kind: "selection", seqs: [3, 1] })).toEqual({
    scope: { scope: "selection", seqs: [3, 1] },
  });

  expect(
    pickExportProgress(
      {
        old: { pct: 100, message: "export: done", at: 10 },
        "export-2": { pct: 30, message: "collecting", at: 50 },
        tunnel: { pct: 90, message: "tunnel", at: 60 },
        other: { pct: 70, message: "Export: writing", at: 55 },
      },
      20,
    ),
  ).toEqual({ pct: 70, message: "Export: writing", at: 55 });
  expect(pickExportProgress({}, 0)).toBeUndefined();

  expect(defaultExportPath("Elia's iPhone 16 Pro", "csv", new Date(2026, 8, 9, 14, 5))).toBe(
    "~/Downloads/StatasMon/elia-s-iphone-16-pro-2026-09-09-1405.csv",
  );
  expect(withExtension("a/b.log", "bundle")).toBe("a/b.zip");
  expect(withExtension("a/b", "csv")).toBe("a/b");
  expect(resultLine({ path: "/p", bytes: 1234567, entries: 10 })).toBe(
    "Wrote 1 234 567 B · 10 entries · /p",
  );
});
