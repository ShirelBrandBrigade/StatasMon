import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { dispatch } from "../bridge/dispatch";
import { synthesize } from "../bridge/synthetic";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { useActiveDevice, useCapability, useLatest, useMetricSeries } from "./useDevice";

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
});

test("hooks read the active device, latest values and series", async () => {
  const msgs = synthesize(3000);
  act(() => {
    for (const { message } of msgs) dispatch(message);
  });
  const { result } = renderHook(() => ({
    active: useActiveDevice(),
    bat: useLatest("fake:iphone", "bat.percent"),
    cap: useCapability("fake:iphone", "cpu_per_core"),
    series: useMetricSeries("fake:iphone", "cpu.total"),
  }));
  expect(result.current.active?.summary.id).toBe("fake:iphone");
  expect(result.current.bat).toBe(87);
  expect(result.current.cap.available).toBeUndefined();
  expect(result.current.series.t.length).toBe(3);
  act(() => {
    dispatch({
      type: "capabilities",
      device_id: "fake:iphone",
      set: {
        entries: [
          {
            capability: "cpu_per_core",
            availability: { status: "unavailable", reason: "Developer Mode is off" },
          },
        ],
      },
    });
  });
  expect(result.current.cap).toEqual({ available: false, reason: "Developer Mode is off" });
});
