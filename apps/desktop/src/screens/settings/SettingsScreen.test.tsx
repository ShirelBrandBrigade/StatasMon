import type { Settings } from "@statasmon/protocol";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../../bridge/commands";
import { SettingsScreen } from "./SettingsScreen";
import { defaultSettings, fromDraft, isDirty, toDraft, validateDraft } from "./settingsModel";

vi.mock("../../bridge/commands", () => ({
  commands: {
    getSettings: vi.fn(),
    setSettings: vi.fn(),
    storeStats: vi.fn(async () => ({
      root: "/Users/dev/Library/Application Support/com.statasmon.app",
      total_bytes: 3 * 2 ** 20,
      session_count: 5,
    })),
  },
}));

const getSettings = vi.mocked(commands.getSettings);
const setSettings = vi.mocked(commands.setSettings);

const STORED: Settings = {
  ...defaultSettings(),
  sampling: { fast_ms: 1000, medium_ms: 3000, slow_ms: 15000, heavy_ms: 0 },
  retention: { max_session_hours: 4, keep_days: 30, max_total_gb: null },
};

beforeEach(() => {
  vi.clearAllMocks();
  getSettings.mockResolvedValue(STORED);
  setSettings.mockImplementation(async (s) => s);
});

async function renderLoaded() {
  render(<SettingsScreen />);
  await waitFor(() =>
    expect(screen.getByTestId("settings-screen")).toHaveAttribute("data-loaded", "true"),
  );
}

test("loads the settings and store stats; Save is disabled until something changes", async () => {
  await renderLoaded();
  expect(getSettings).toHaveBeenCalledTimes(1);
  expect(screen.getByLabelText("Fast")).toHaveValue("1000");
  expect(screen.getByLabelText("Heavy")).toHaveValue("0");
  expect(screen.getByLabelText("Days to keep")).toHaveValue("30");
  expect(screen.getByLabelText("Total size cap")).toHaveValue("");
  expect(screen.getByLabelText("adb path")).toHaveValue("");
  expect(screen.getByRole("checkbox", { name: /Check for updates/ })).not.toBeChecked();
  const save = screen.getByRole("button", { name: "Save" });
  expect(save).toBeDisabled();
  expect(screen.getByTestId("settings-screen")).toHaveAttribute("data-dirty", "false");

  const storage = within(screen.getByRole("region", { name: "Storage" }));
  expect(storage.getByText("3.0 MB")).toBeInTheDocument();
  expect(storage.getByText("5")).toBeInTheDocument();
  expect(storage.getByText(/com\.statasmon\.app/)).toBeInTheDocument();
  // Browser build: no native picker, no Reveal.
  expect(screen.queryByRole("button", { name: "Choose…" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Reveal" })).toBeNull();
});

test("editing makes it dirty, Revert restores, Save sends the edited values", async () => {
  await renderLoaded();
  const fast = screen.getByLabelText("Fast");
  fireEvent.change(fast, { target: { value: "500" } });
  expect(screen.getByTestId("settings-screen")).toHaveAttribute("data-dirty", "true");
  const save = screen.getByRole("button", { name: "Save" });
  expect(save).toBeEnabled();

  fireEvent.click(screen.getByRole("button", { name: "Revert" }));
  expect(fast).toHaveValue("1000");
  expect(save).toBeDisabled();

  fireEvent.change(fast, { target: { value: "500" } });
  fireEvent.change(screen.getByLabelText("Days to keep"), { target: { value: "7" } });
  fireEvent.change(screen.getByLabelText("Total size cap"), { target: { value: "2.5" } });
  fireEvent.change(screen.getByLabelText("adb path"), { target: { value: "  /opt/adb  " } });
  fireEvent.click(screen.getByRole("checkbox", { name: /Check for updates/ }));
  await act(async () => {
    fireEvent.click(save);
  });
  expect(setSettings).toHaveBeenCalledTimes(1);
  expect(setSettings).toHaveBeenCalledWith({
    ...STORED,
    sampling: { ...STORED.sampling, fast_ms: 500 },
    retention: { max_session_hours: 4, keep_days: 7, max_total_gb: 2.5 },
    adb_path: "/opt/adb",
    update_check: true,
  });
  expect(screen.getByRole("status")).toHaveTextContent("Saved");
  expect(screen.getByTestId("settings-screen")).toHaveAttribute("data-dirty", "false");
  expect(save).toBeDisabled();
  expect(screen.getByLabelText("adb path")).toHaveValue("/opt/adb");
});

test("inline validation names the bound and blocks the save", async () => {
  await renderLoaded();
  fireEvent.change(screen.getByLabelText("Fast"), { target: { value: "100" } });
  fireEvent.change(screen.getByLabelText("Heavy"), { target: { value: "1000" } });
  fireEvent.change(screen.getByLabelText("Hours per session"), { target: { value: "49" } });
  fireEvent.change(screen.getByLabelText("Total size cap"), { target: { value: "-1" } });
  expect(screen.getByText("Fast must be a whole number of at least 250 ms.")).toBeInTheDocument();
  expect(screen.getByText("Heavy must be 0 (off) or at least 30000 ms.")).toBeInTheDocument();
  expect(screen.getByText("Hours per session must be from 1 to 48.")).toBeInTheDocument();
  expect(
    screen.getByText("Total size must be a positive number of GB, or empty for no cap."),
  ).toBeInTheDocument();
  expect(screen.getByLabelText("Fast")).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByLabelText("Medium")).not.toHaveAttribute("aria-invalid");

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
  });
  expect(setSettings).not.toHaveBeenCalled();
  expect(screen.getByRole("status")).toHaveTextContent("Fix the highlighted fields first");
});

test("a core rejection is shown and the draft is kept", async () => {
  setSettings.mockRejectedValueOnce({
    code: "unsupported",
    message: "adb was not found at /nope/adb",
  });
  await renderLoaded();
  fireEvent.change(screen.getByLabelText("adb path"), { target: { value: "/nope/adb" } });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
  });
  expect(screen.getByRole("status")).toHaveTextContent("adb was not found at /nope/adb");
  expect(screen.getByLabelText("adb path")).toHaveValue("/nope/adb");
  expect(screen.getByTestId("settings-screen")).toHaveAttribute("data-dirty", "true");
});

test("a failed load is reported, not hidden", async () => {
  getSettings.mockRejectedValueOnce(new Error("no core"));
  render(<SettingsScreen />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Settings could not be loaded: no core",
  );
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
});

test("model: draft round trip, dirty comparison and bounds", () => {
  const s = defaultSettings();
  const d = toDraft(s);
  expect(validateDraft(d)).toEqual({});
  expect(isDirty(d, s)).toBe(false);
  expect(fromDraft(d, s)).toEqual(s);
  expect(isDirty({ ...d, adb_path: "  " }, s)).toBe(false);
  expect(isDirty({ ...d, update_check: true }, s)).toBe(true);
  expect(validateDraft({ ...d, fast_ms: "249" }).fast_ms).toMatch(/250/);
  expect(validateDraft({ ...d, fast_ms: "250" }).fast_ms).toBeUndefined();
  expect(validateDraft({ ...d, medium_ms: "abc" }).medium_ms).toMatch(/whole number/);
  expect(validateDraft({ ...d, slow_ms: "4999" }).slow_ms).toMatch(/5000/);
  expect(validateDraft({ ...d, heavy_ms: "30000" }).heavy_ms).toBeUndefined();
  expect(validateDraft({ ...d, keep_days: "0" }).keep_days).toMatch(/1 to 365/);
  expect(validateDraft({ ...d, keep_days: "366" }).keep_days).toMatch(/1 to 365/);
  expect(validateDraft({ ...d, max_total_gb: "0" }).max_total_gb).toMatch(/positive/);
  expect(validateDraft({ ...d, max_total_gb: "1.5" }).max_total_gb).toBeUndefined();
  expect(fromDraft({ ...d, max_total_gb: "1.5", heavy_ms: "60000" }, s)).toEqual({
    ...s,
    sampling: { ...s.sampling, heavy_ms: 60000 },
    retention: { ...s.retention, max_total_gb: 1.5 },
  });
});
