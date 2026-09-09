import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bridge/commands";
import { dispatch } from "../bridge/dispatch";
import { synthesize } from "../bridge/synthetic";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { formatElapsed } from "./format";
import { RecordingCard } from "./RecordingCard";

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

beforeEach(() => {
  vi.useFakeTimers();
  useAppStore.getState().reset();
  telemetry.reset();
  act(() => {
    for (const { message } of synthesize(3000)) dispatch(message);
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});

test("shows Recording, a ticking elapsed time and the entry count; Pause calls the core", () => {
  render(<RecordingCard deviceId="fake:iphone" />);
  expect(screen.getByText("Recording")).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "Recording" })).toBeInTheDocument();
  const time = screen.getByText("00:00");
  expect(time.closest(".mono")).not.toBeNull();
  expect(screen.getByText(/9 entries/)).toBeInTheDocument();

  act(() => {
    vi.advanceTimersByTime(2000);
  });
  expect(screen.getByText("00:02")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Pause" }));
  expect(commands.pauseSession).toHaveBeenCalledWith("fake:iphone");

  fireEvent.click(screen.getByRole("button", { name: "Mark" }));
  expect(commands.mark).toHaveBeenCalledWith("fake:iphone", "Mark 1");
});

test("resyncs to the elapsed_ms the core reports and stops ticking when paused", () => {
  render(<RecordingCard deviceId="fake:iphone" />);
  act(() => {
    dispatch({
      type: "session",
      device_id: "fake:iphone",
      session: {
        id: "s",
        status: "paused",
        started_at_ms: 0,
        elapsed_ms: 3_725_000,
        sample_count: 0,
        log_count: 0,
        marks: [],
      },
    });
  });
  expect(screen.getByText("Paused")).toBeInTheDocument();
  expect(screen.getByText("1:02:05")).toBeInTheDocument();
  act(() => {
    vi.advanceTimersByTime(3000);
  });
  expect(screen.getByText("1:02:05")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Resume" }));
  expect(commands.resumeSession).toHaveBeenCalledWith("fake:iphone");
});

test("formatElapsed", () => {
  expect(formatElapsed(0)).toBe("00:00");
  expect(formatElapsed(61_000)).toBe("01:01");
  expect(formatElapsed(3_600_000)).toBe("1:00:00");
});
