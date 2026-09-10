import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { dispatch } from "../../bridge/dispatch";
import { useExportStore } from "../../export/exportStore";
import { useAppStore } from "../../store/app";
import { useLogFilterStore } from "../../store/logFilter";
import { telemetry } from "../../store/telemetry";
import { LogCard } from "./LogCard";
import { mockupEntries } from "./testEntries";

const ID = "fake:iphone";

beforeEach(() => {
  useAppStore.getState().reset();
  useLogFilterStore.getState().reset();
  telemetry.reset();
  dispatch({
    type: "device_attached",
    device: {
      id: ID,
      platform: "ios",
      name: "iPhone 16 Pro",
      model_name: "iPhone 16 Pro",
      transport: "usb",
      state: { state: "tier2" },
    },
  });
});

afterEach(() => vi.useRealTimers());

function seed() {
  act(() => dispatch({ type: "log", device_id: ID, entries: mockupEntries() }));
}

const list = () => screen.getByRole("listbox");
const rows = () => Array.from(document.querySelectorAll<HTMLElement>(".log-row"));
const button = (name: string) => screen.getByRole("button", { name });

test("renders rows from the seeded store with the default level filter", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  expect(list().dataset.count).toBe("7"); // debug hidden
  const r = rows();
  expect(r).toHaveLength(7);
  expect(r[0].querySelector(".log-time")?.textContent).toMatch(/^\d\d:\d\d:\d\d\.\d\d\d$/);
  expect(r[0].querySelector(".log-proc")?.textContent).toBe("FieldKit");
  expect(r[0].querySelector(".sm-tile")?.getAttribute("data-me")).toBe("true");
  expect(r[1].querySelector(".sm-tile")?.getAttribute("data-me")).toBeNull();
  expect(r[0].querySelector(".log-msg")?.textContent).toContain("Session started");
  expect(r[1]).toHaveClass("is-alt");
  expect(screen.getByText("Retry 1/3 in 2.0 s")).toBeInTheDocument();
  expect(screen.queryByText(/Resource load completed/)).not.toBeInTheDocument();
});

test("highlights fault rows and marks the Android app id as the app under test", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  const fault = document.querySelector<HTMLElement>('.log-row[data-level="fault"]');
  expect(fault).toHaveClass("is-fault");
  expect(fault?.querySelector(".sm-pill")?.textContent).toBe("Fault");
  const android = document.querySelector<HTMLElement>('.log-row[data-seq="8"]');
  expect(android?.querySelector(".sm-tile")?.getAttribute("data-me")).toBe("true");
  expect(rows().filter((r) => r.classList.contains("is-fault"))).toHaveLength(1);
});

test("level toggles hide rows; W appears only when warnings were seen", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  const error = button("Error");
  expect(error).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(error);
  expect(error).toHaveAttribute("aria-pressed", "false");
  expect(list().dataset.count).toBe("6");
  expect(screen.queryByText(/URLSession task failed/)).not.toBeInTheDocument();
  expect(button("Warning")).toBeInTheDocument();
  fireEvent.click(button("Debug"));
  expect(list().dataset.count).toBe("7");
  expect(screen.getByText(/Resource load completed/)).toBeInTheDocument();
});

test("no W toggle before any warning line", () => {
  act(() => dispatch({ type: "log", device_id: ID, entries: mockupEntries().slice(0, 3) }));
  render(<LogCard deviceId={ID} initialItemCount={3} />);
  expect(screen.queryByRole("button", { name: "Warning" })).not.toBeInTheDocument();
  expect(
    Array.from(document.querySelectorAll<HTMLElement>(".log-lvl")).map((b) => b.textContent),
  ).toEqual(["D", "I", "N", "E", "F"]);
});

test("app pills: only the selection gets a pill; the picker selects, adds and clears", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  const pills = () =>
    Array.from(screen.getByRole("toolbar").querySelectorAll<HTMLButtonElement>(".log-pill")).map(
      (b) => b.textContent,
    );
  expect(pills()).toEqual(["All apps", "+ app", "+ filter"]);
  expect(button("All apps")).toHaveAttribute("aria-pressed", "true");

  fireEvent.click(button("Filter by app"));
  const dialog = screen.getByRole("dialog", { name: "Filter by app" });
  expect(
    within(dialog)
      .getAllByRole("option")
      .map((o) => o.getAttribute("data-process")),
  ).toEqual(["FieldKit", "com.example.fieldkit", "Safari", "SpringBoard"]);
  fireEvent.click(within(dialog).getByRole("option", { name: /SpringBoard/ }));
  expect(button("All apps")).toHaveAttribute("aria-pressed", "false");
  expect(pills()).toEqual(["All apps", "SpringBoard", "+ app", "+ filter"]);
  expect(button("SpringBoard")).toHaveAttribute("aria-pressed", "true");
  expect(list().dataset.count).toBe("1");

  fireEvent.click(button("Filter by app"));
  fireEvent.click(screen.getByRole("option", { name: /FieldKit/ }), { shiftKey: true });
  expect(list().dataset.count).toBe("5");
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Clear" }));
  expect(list().dataset.count).toBe("7");
  expect(button("All apps")).toHaveAttribute("aria-pressed", "true");
  // Clicking a selected pill removes it from the selection.
  act(() => useLogFilterStore.getState().setProcesses(ID, ["thermalmonitord"]));
  expect(pills()).toContain("thermalmonitord");
  expect(list().dataset.count).toBe("1");
  fireEvent.click(button("thermalmonitord"));
  expect(list().dataset.count).toBe("7");
});

test("free-text and regex search, invalid regex note, Escape clears", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  fireEvent.click(button("Add a text filter"));
  const input = screen.getByRole("searchbox", { name: "Search log" });
  fireEvent.change(input, { target: { value: "RETRY" } });
  expect(list().dataset.count).toBe("1");
  fireEvent.change(input, { target: { value: "/(/" } });
  expect(list().dataset.count).toBe("0");
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByText("invalid regex")).toBeInTheDocument();
  expect(screen.getByText("No entries match the filter")).toBeInTheDocument();
  fireEvent.keyDown(input, { key: "Escape" });
  expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  expect(list().dataset.count).toBe("7");
});

test("footer counts, follow-tail badge and Space toggle", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  const footer = document.querySelector(".sm-card-footer");
  expect(footer?.textContent).toContain("8 entries");
  expect(footer?.textContent).toContain("7 shown");
  expect(footer?.textContent).toContain("1 error · 1 fault");
  expect(list().dataset.follow).toBe("following");
  fireEvent.keyDown(list(), { key: " " });
  expect(list().dataset.follow).toBe("paused");
  expect(useLogFilterStore.getState().byDevice[ID]?.followTail).toBe(false);
  expect(footer?.textContent).toContain("paused");
  fireEvent.keyDown(list(), { key: " " });
  expect(useLogFilterStore.getState().byDevice[ID]?.followTail).toBe(true);
});

test("clicking a row toggles its selection and the footer counts it", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  const row = rows()[2];
  fireEvent.click(row);
  expect(row).toHaveClass("is-selected");
  expect(row).toHaveAttribute("aria-selected", "true");
  expect(document.querySelector(".sm-card-footer")?.textContent).toContain("1 selected");
  fireEvent.click(row);
  expect(row).not.toHaveClass("is-selected");
});

test("Export opens the export dialog scoped to the filter, or to the selection", () => {
  seed();
  render(<LogCard deviceId={ID} initialItemCount={7} />);
  fireEvent.click(button("Export log (.log, .json)"));
  expect(useExportStore.getState().open).toBe(true);
  expect(useExportStore.getState().preset.scope).toBe("filter");
  useExportStore.getState().close();
  fireEvent.click(screen.getAllByRole("option")[0]);
  fireEvent.click(button("Export selection (.log, .json)"));
  expect(useExportStore.getState().preset.scope).toBe("selection");
  expect(useExportStore.getState().preset.seqs?.length).toBe(1);
});

test("shows a Dash with the reason when the device cannot provide logs", () => {
  act(() =>
    dispatch({
      type: "capabilities",
      device_id: ID,
      set: {
        entries: [
          { capability: "logs", availability: { status: "unavailable", reason: "Not paired" } },
        ],
      },
    }),
  );
  render(<LogCard deviceId={ID} />);
  expect(screen.getByRole("img", { name: "Unavailable: Not paired" })).toBeInTheDocument();
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
});
