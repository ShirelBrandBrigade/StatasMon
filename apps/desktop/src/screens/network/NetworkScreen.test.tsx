import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../../bridge/commands";
import { ME_APP_ID } from "../../overview/cards/shared";
import {
  ANDROID,
  caps,
  fresh,
  IPHONE,
  proc,
  processes,
  sample,
  seed,
} from "../../overview/cards/testkit";
import { defaultCapturePath, describeActionError, slug, useCaptureStore } from "./captureStore";
import { NetworkScreen } from "./index";

vi.mock("../../bridge/commands", () => ({
  commands: { queryHistory: vi.fn(async () => []), deviceAction: vi.fn(async () => null) },
}));

const deviceAction = vi.mocked(commands.deviceAction);
const WIFI_REASON = "iOS does not expose Wi-Fi signal details to the host";
const ROOT_REASON = "Needs root on Android";

function seedIphone(): void {
  caps(IPHONE, {
    network_throughput: true,
    network_packets: true,
    network_per_process: true,
    wifi_details: WIFI_REASON,
    packet_capture: true,
  });
  sample(IPHONE, {
    "net.rx_bps": 24.6e6,
    "net.tx_bps": 3.2e6,
    "net.rx_pps": 1842,
    "net.tx_pps": 611,
    "net.rx_total": 1.8 * 2 ** 30,
    "net.tx_total": 214 * 2 ** 20,
    "net.connections": 27,
    "net.retransmits": 12,
    "net.dropped": 0,
    "net.latency_ms": 18,
  });
  processes(IPHONE, [
    proc({ pid: 3, name: "kernel_task" }),
    proc({ pid: 2, name: "Safari", app_id: "com.apple.mobilesafari", net_bps: 6.6e5 }),
    proc({ pid: 1, name: "FieldKit", app_id: ME_APP_ID, net_bps: 2.3e6 }),
    proc({ pid: 4, name: "locationd", net_bps: 0 }),
  ]);
}

beforeEach(() => {
  seed();
  useCaptureStore.getState().reset();
  vi.clearAllMocks();
});

test("throughput figures, packets and totals, both charts", () => {
  act(() => seedIphone());
  render(<NetworkScreen deviceId={IPHONE} />);

  const card = within(screen.getByTestId("net-throughput"));
  expect(within(card.getByTestId("net-down")).getByTestId("figure-number")).toHaveTextContent(
    "24.6",
  );
  expect(card.getByText("Mb/s down")).toBeInTheDocument();
  expect(card.getByTestId("net-down")).toHaveTextContent("1 842 pkt/s");
  expect(within(card.getByTestId("net-up")).getByTestId("figure-number")).toHaveTextContent("3.2");
  expect(card.getByTestId("net-up")).toHaveTextContent("611 pkt/s");
  expect(card.getByText("1.8 GB down · 214.0 MB up this session")).toBeInTheDocument();
  expect(
    card.getByRole("img", { name: "Download and upload throughput over time" }),
  ).toBeInTheDocument();
  expect(
    card.getByRole("img", { name: "Packets in and out per second over time" }),
  ).toBeInTheDocument();
});

test("interface rows: Wi-Fi details dash with the iOS reason, counters and latency show", () => {
  act(() => seedIphone());
  render(<NetworkScreen deviceId={IPHONE} />);

  const kv = screen.getByTestId("net-interface-kv");
  expect(within(kv).getAllByRole("img", { name: `Unavailable: ${WIFI_REASON}` })).toHaveLength(4);
  expect(kv).toHaveTextContent("Connections27");
  expect(kv).toHaveTextContent("Retransmits12");
  expect(kv).toHaveTextContent("Dropped0");
  expect(kv).toHaveTextContent("Latency18 ms");
  expect(screen.getByTestId("net-interface")).toHaveTextContent("host link USB");
});

test("interface rows on Android: RSSI with a typographic minus and the link speed", () => {
  act(() => {
    caps(ANDROID, { wifi_details: true, network_throughput: true, network_per_process: true });
    sample(ANDROID, { "net.rssi": -56.3, "net.link_mbps": 866, "net.latency_ms": 21.9 });
  });
  render(<NetworkScreen deviceId={ANDROID} />);
  const kv = screen.getByTestId("net-interface-kv");
  expect(kv).toHaveTextContent("Signal (RSSI)−56.3 dBm");
  expect(kv).toHaveTextContent("Link speed866 Mb/s");
  expect(kv).toHaveTextContent("Latency21.9 ms");
  expect(
    within(kv).getAllByRole("img", { name: "Unavailable: Not in the data contract yet" }),
  ).toHaveLength(2);
});

test("per-process traffic: rows with a rate, busiest first, mono rates and the app tile", () => {
  act(() => seedIphone());
  render(<NetworkScreen deviceId={IPHONE} />);

  const rows = within(screen.getByTestId("net-processes")).getAllByTestId("net-proc-row");
  expect(rows).toHaveLength(3);
  expect(rows[0]).toHaveTextContent("FieldKit");
  expect(rows[0]).toHaveTextContent("2.3 Mb/s");
  expect(within(rows[0]).getByRole("img", { name: ME_APP_ID })).toBeInTheDocument();
  expect(rows[1]).toHaveTextContent("Safari");
  expect(rows[1]).toHaveTextContent("660.0 kb/s");
  expect(rows[2]).toHaveTextContent("locationd");
  expect(screen.getByTestId("net-processes")).toHaveTextContent("3 processes");
});

test("per-process traffic dashes with the capability reason when no row carries a rate", () => {
  const reason = "Per-process traffic needs the DVT networking service";
  act(() => {
    seedIphone();
    caps(IPHONE, { network_per_process: reason });
    processes(IPHONE, [proc({ pid: 3, name: "kernel_task" })]);
  });
  render(<NetworkScreen deviceId={IPHONE} />);
  const card = within(screen.getByTestId("net-processes"));
  expect(card.queryAllByTestId("net-proc-row")).toHaveLength(0);
  expect(card.getByRole("img", { name: `Unavailable: ${reason}` })).toBeInTheDocument();
});

test("packet capture: prefilled path, start → capturing since, stop → idle", async () => {
  act(() => seedIphone());
  render(<NetworkScreen deviceId={IPHONE} />);

  const input = screen.getByLabelText("Save to") as HTMLInputElement;
  expect(input.value).toMatch(
    /^~\/Downloads\/StatasMon\/iphone-16-pro-\d{4}-\d{2}-\d{2}-\d{6}\.pcap$/,
  );
  expect(screen.getByTestId("pcap-state")).toHaveTextContent("Idle");
  expect(screen.queryByRole("button", { name: "Choose…" })).toBeNull();

  fireEvent.change(input, { target: { value: "~/Downloads/StatasMon/custom.pcap" } });
  fireEvent.click(screen.getByRole("button", { name: "Start capture" }));
  await waitFor(() =>
    expect(screen.getByTestId("pcap-state")).toHaveTextContent(/Capturing since \d{2}:\d{2}:\d{2}/),
  );
  expect(deviceAction).toHaveBeenCalledWith(IPHONE, {
    action: "pcap_start",
    path: "~/Downloads/StatasMon/custom.pcap",
  });
  expect(screen.getByTestId("pcap-elapsed")).toHaveTextContent("custom.pcap");
  expect(input).toBeDisabled();

  fireEvent.click(screen.getByRole("button", { name: "Stop capture" }));
  await waitFor(() => expect(screen.getByTestId("pcap-state")).toHaveTextContent("Idle"));
  expect(deviceAction).toHaveBeenLastCalledWith(IPHONE, { action: "pcap_stop" });
  expect(screen.getByRole("button", { name: "Start capture" })).toBeEnabled();
});

test("packet capture: a rejected start shows the code, message and hint inline", async () => {
  act(() => seedIphone());
  deviceAction.mockRejectedValueOnce({
    code: "not_trusted",
    message: "pcapd refused the connection",
    hint: "Unlock the iPhone and tap Trust.",
  });
  render(<NetworkScreen deviceId={IPHONE} />);

  fireEvent.click(screen.getByRole("button", { name: "Start capture" }));
  const error = await screen.findByTestId("pcap-error");
  expect(error).toHaveTextContent("not_trusted · pcapd refused the connection");
  expect(error).toHaveTextContent("Unlock the iPhone and tap Trust.");
  expect(screen.getByTestId("pcap-state")).toHaveTextContent("Idle");
  expect(screen.getByRole("button", { name: "Start capture" })).toBeEnabled();
});

test("packet capture is dashed with the reason and disabled where the platform lacks it", () => {
  act(() => {
    fresh(ANDROID, "android");
    caps(ANDROID, { packet_capture: ROOT_REASON });
  });
  render(<NetworkScreen deviceId={ANDROID} />);

  expect(screen.getByTestId("pcap-off")).toHaveTextContent(ROOT_REASON);
  expect(
    within(screen.getByTestId("pcap-state")).getByRole("img", {
      name: `Unavailable: ${ROOT_REASON}`,
    }),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Start capture" })).toBeDisabled();
  expect(screen.getByLabelText("Save to")).toBeDisabled();
});

test("capture path helpers: slug and default path", () => {
  expect(slug("iPhone 16 Pro")).toBe("iphone-16-pro");
  expect(slug("Galaxy Tab S8 (Wi-Fi)")).toBe("galaxy-tab-s8-wi-fi");
  expect(slug("***")).toBe("device");
  expect(defaultCapturePath("Studio iPad", new Date(2026, 8, 9, 14, 5, 7))).toBe(
    "~/Downloads/StatasMon/studio-ipad-2026-09-09-140507.pcap",
  );
  expect(describeActionError("boom")).toEqual({ code: "internal", message: "boom" });
  expect(describeActionError(new Error("nope"))).toMatchObject({
    code: "internal",
    message: "nope",
  });
  expect(describeActionError({ code: "unsupported", message: "m", hint: "h" })).toEqual({
    code: "unsupported",
    message: "m",
    hint: "h",
  });
});
