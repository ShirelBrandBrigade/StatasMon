import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { Message } from "@statasmon/protocol";
import { beforeEach, expect, test } from "vitest";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { dispatch } from "./dispatch";

const FIXTURE = resolve(__dirname, "../../../../fixtures/fake/session-3dev-30s.jsonl");

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
});

test("the recorded 3-device session replays into the store and mirrors", () => {
  const lines = readFileSync(FIXTURE, "utf8")
    .split("\n")
    .filter((l) => l.trim());
  for (const l of lines) dispatch(JSON.parse(l) as Message);
  const s = useAppStore.getState();
  expect(s.order.sort()).toEqual(["fake:android", "fake:ipad", "fake:iphone"]);
  for (const id of ["fake:iphone", "fake:android"]) {
    expect(telemetry.metricsFor(id).size, `${id} samples`).toBeGreaterThan(20);
    expect(telemetry.cores.get(id)?.length, `${id} cores`).toBeGreaterThan(0);
    expect(telemetry.processes.get(id)?.length, `${id} processes`).toBeGreaterThan(0);
    expect(telemetry.logsFor(id).size, `${id} logs`).toBeGreaterThan(50);
    expect(s.devices[id].headline?.sentence).toBeTruthy();
    expect(s.devices[id].session?.status).toBe("recording");
  }
});
