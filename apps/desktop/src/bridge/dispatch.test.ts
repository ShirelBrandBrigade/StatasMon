import { beforeEach, describe, expect, test } from "vitest";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { dispatch } from "./dispatch";
import { synthesize } from "./synthetic";

describe("dispatch", () => {
  beforeEach(() => {
    useAppStore.getState().reset();
    telemetry.reset();
  });

  test("routes a synthetic session into the store and the mirror buffers", () => {
    for (const { message } of synthesize(3000)) dispatch(message);
    const s = useAppStore.getState();
    expect(s.order).toEqual(["fake:iphone", "fake:ipad", "fake:android"]);
    expect(s.activeDeviceId).toBe("fake:iphone");
    const d = s.devices["fake:android"];
    expect(d.headline?.severity).toBe("nominal");
    expect(d.session?.status).toBe("recording");
    expect(d.latest["bat.percent"]).toBe(87);
    expect(telemetry.metricsFor("fake:iphone").size).toBe(3);
    expect(telemetry.logsFor("fake:iphone").size).toBe(9);
    expect(telemetry.cores.get("fake:ipad")?.length).toBe(6);
  });

  test("detach removes the device and its buffers", () => {
    for (const { message } of synthesize(1000)) dispatch(message);
    dispatch({ type: "device_detached", device_id: "fake:iphone" });
    const s = useAppStore.getState();
    expect(s.order).toEqual(["fake:ipad", "fake:android"]);
    expect(s.activeDeviceId).toBe("fake:ipad");
    expect(telemetry.metrics.has("fake:iphone")).toBe(false);
  });
});
