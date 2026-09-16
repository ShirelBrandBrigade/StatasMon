import type { Capability } from "@statasmon/protocol";
import { act, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { dispatch } from "../../bridge/dispatch";
import { synthesize } from "../../bridge/synthetic";
import { installCanvasStub } from "../../charts/testing/jsdomCanvas";
import { useAppStore } from "../../store/app";
import { telemetry } from "../../store/telemetry";

// uPlot (via TimeSeriesChart) touches matchMedia and the canvas at import time.
installCanvasStub();
const { NetworkCard } = await import("./NetworkCard");

const DEVICE = "fake:iphone";
const NET_CAPS: Capability[] = [
  "network_throughput",
  "network_packets",
  "network_per_process",
  "wifi_details",
];

function seedCapabilities(unavailable: Partial<Record<Capability, string>> = {}) {
  dispatch({
    type: "capabilities",
    device_id: DEVICE,
    set: {
      entries: NET_CAPS.map((capability) => {
        const reason = unavailable[capability];
        return {
          capability,
          availability: reason ? { status: "unavailable", reason } : { status: "available" },
        };
      }),
    },
  });
}

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
});

test("renders formatted figures, captions, side counters, subtitle and the chart from a seeded store", () => {
  act(() => {
    for (const { message } of synthesize(3000)) dispatch(message);
    seedCapabilities();
    useAppStore.getState().setLatest(DEVICE, 1_757_419_203_000, {
      "net.rx_bps": 24.6e6,
      "net.tx_bps": 3.2e6,
      "net.rx_pps": 1842,
      "net.tx_pps": 611,
      "net.rx_total": 1.8 * 2 ** 30,
      "net.tx_total": 214 * 2 ** 20,
      "net.rssi": -58,
      "net.latency_ms": 18,
      "net.dropped": 0,
      "net.retransmits": 12,
      "net.connections": 27,
    });
  });
  const { container } = render(<NetworkCard deviceId={DEVICE} />);

  expect(screen.getByText("Network")).toBeInTheDocument();
  expect(screen.getByTestId("net-subtitle")).toHaveTextContent("Wi-Fi · −58 dBm · 18 ms");

  const down = within(screen.getByTestId("net-down"));
  expect(down.getByTestId("figure-number")).toHaveTextContent("24.6");
  expect(down.getByText("Mb/s down")).toBeInTheDocument();
  expect(screen.getByTestId("net-down-caption")).toHaveTextContent("1 842 pkt/s · 1.8 GB");

  const up = within(screen.getByTestId("net-up"));
  expect(up.getByTestId("figure-number")).toHaveTextContent("3.2");
  expect(up.getByText("Mb/s up")).toBeInTheDocument();
  expect(screen.getByTestId("net-up-caption")).toHaveTextContent("611 pkt/s · 214.0 MB");

  const side = screen.getByTestId("net-side");
  expect(side).toHaveTextContent("0 dropped · 12 retransmits");
  expect(side).toHaveTextContent("27 connections");

  // The chart is mounted once with the two series from the telemetry mirror (3 samples).
  expect(container.querySelectorAll(".uplot").length).toBe(1);
  expect(screen.queryByTestId("net-chart-off")).toBeNull();
  expect(container.querySelector('[role="img"][aria-label]')).toHaveAttribute(
    "aria-label",
    "Download and upload throughput over time",
  );
  expect(screen.queryByRole("img", { name: /Unavailable/ })).toBeNull();
});

test("shows dashes with the reason when network_throughput is unavailable", () => {
  const reason = "Developer Mode is off on this iPad";
  act(() => {
    // Attach without samples: an unavailable capability never produces values.
    for (const { message } of synthesize(0)) dispatch(message);
    seedCapabilities({ network_throughput: reason, wifi_details: "no wifi details" });
  });
  const { container } = render(<NetworkCard deviceId={DEVICE} />);

  // Down, up, two totals and the chart placeholder carry the throughput
  // reason; the subtitle's dash carries the Wi-Fi reason instead.
  const dashes = screen.getAllByLabelText(`Unavailable: ${reason}`);
  expect(dashes.length).toBe(5);
  expect(within(screen.getByTestId("net-down")).getByText("down")).toBeInTheDocument();
  expect(screen.getByTestId("net-chart-off")).toHaveTextContent(reason);
  expect(container.querySelector(".uplot")).toBeNull();
  // Other capabilities keep their own reasons.
  expect(screen.getByLabelText("Unavailable: no wifi details")).toBeInTheDocument();
  // Packets are available but not yet sampled: honest interim text, not the throughput reason.
  expect(screen.getAllByLabelText("Unavailable: Not reported yet").length).toBe(5);
});

test("before the capability list arrives, missing values explain that they are waiting", () => {
  act(() => {
    for (const { message } of synthesize(0)) dispatch(message);
  });
  render(<NetworkCard deviceId={DEVICE} />);
  expect(
    screen.getAllByLabelText("Unavailable: Waiting for the device's capability list").length,
  ).toBeGreaterThan(0);
});
