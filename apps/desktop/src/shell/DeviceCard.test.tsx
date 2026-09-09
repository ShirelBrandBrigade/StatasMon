import type { DeviceSummary } from "@statasmon/protocol";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import type { DeviceEntry } from "../store/app";
import { DeviceCard, deviceStateLine, NEEDS_TRUST_TEXT, transportLabel } from "./DeviceCard";

afterEach(cleanup);

function entry(state: DeviceSummary["state"], transport: DeviceSummary["transport"] = "usb") {
  const summary: DeviceSummary = {
    id: "android:abc",
    platform: "android",
    name: "Pixel 8",
    model_name: "Pixel 8",
    transport,
    state,
  };
  const e: DeviceEntry = { summary, latest: {}, eventCount: 0, errorCount: 0 };
  return e;
}

test("needs_trust shows the USB debugging prompt line with a warn dot", () => {
  render(<DeviceCard entry={entry({ state: "needs_trust" })} active={false} onSelect={() => {}} />);
  expect(screen.getByText(NEEDS_TRUST_TEXT)).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "Needs trust" })).toHaveAttribute("data-tone", "warn");
  expect(screen.getByRole("button")).toHaveAttribute("data-state", "needs_trust");
});

test("connecting and error states, and the transport labels", () => {
  expect(deviceStateLine(entry({ state: "connecting" }, "adb_tcp").summary).text).toBe(
    "Wi-Fi ADB · Connecting…",
  );
  const err = deviceStateLine(
    entry({ state: "error", code: "io", message: "shell exited" }).summary,
  );
  expect(err).toEqual({ text: "shell exited", tone: "bad", label: "Error" });
  expect(deviceStateLine(entry({ state: "tier1" }).summary, "recording").text).toBe(
    "USB · recording",
  );
  expect(transportLabel("adb_tcp")).toBe("Wi-Fi ADB");
  expect(transportLabel("usb")).toBe("USB");
  expect(transportLabel("wifi")).toBe("Wi-Fi");
  expect(transportLabel("fake")).toBe("Fake");
});
