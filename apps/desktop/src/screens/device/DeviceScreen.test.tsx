import type { CapabilitySet, DeviceInfo, DeviceSummary } from "@statasmon/protocol";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../../bridge/commands";
import { useAppStore } from "../../store/app";
import { DeviceScreen } from "./DeviceScreen";
import {
  capabilityLabel,
  formatAge,
  formatUptime,
  hardwareRows,
  rawJson,
  sortedRaw,
} from "./deviceModel";

vi.mock("../../bridge/commands", () => ({
  commands: { deviceAction: vi.fn(async () => ({ outcome: "done" })) },
}));

const IPHONE: DeviceSummary = {
  id: "fake:iphone",
  platform: "ios",
  name: "Elia's iPhone",
  model_name: "iPhone 16 Pro",
  transport: "usb",
  state: { state: "tier2" },
};

const INFO: DeviceInfo = {
  identity: {
    name: "Elia's iPhone",
    marketing_name: "iPhone 16 Pro",
    model_identifier: "iPhone17,1",
    model_number: "MYNA3LL/A",
    manufacturer: "Apple",
    serial_number: "F2LXFAKE0001",
    unique_id: "00008140-000A2C3E0E80001E",
    region: "LL/A",
  },
  software: {
    os_name: "iOS",
    os_version: "27.0",
    os_build: "25A354",
    security_patch: null,
    kernel_version: "Darwin 25.0.0",
    api_level: null,
    firmware_version: "1.00.03",
  },
  hardware: {
    chip_name: "A18 Pro",
    chip_identifier: "t8140",
    chip_manufacturer: "Apple",
    architecture: "arm64e",
    abis: ["arm64e", "arm64"],
    cpu_cores: 6,
    performance_cores: 2,
    efficiency_cores: 4,
    cpu_max_mhz: null,
    gpu_name: "Apple GPU (6-core)",
    ram_bytes: 8 * 2 ** 30,
    storage_bytes: 256_000_000_000,
    battery_design_mah: 3582,
    board: "D93AP",
    screen: "2622×1206 @ 460 ppi",
  },
  state: {
    uptime_secs: 263_320,
    boot_time_ms: 1_788_687_494_819,
    time_zone: "Europe/Paris",
    locale: "en_US",
    developer_mode: true,
    usb_debugging: null,
    wireless_debugging: null,
    selinux_mode: null,
  },
  connection: {
    transport: "usb",
    tier: "tier 2",
    pairing: "trusted",
    tunnel: "userspace tunnel up",
    last_heartbeat_ms: Date.now() - 12_000,
    tools: [{ name: "statasmon-device-fake", version: "0.1.0" }],
  },
  raw: {
    ProductType: "iPhone17,1",
    ChipID: "0x8140",
    HardwareModel: "D93AP",
    ActivationState: "Activated",
  },
};

const CAPS: CapabilitySet = {
  entries: [
    { capability: "cpu_per_core", availability: { status: "available" } },
    {
      capability: "cpu_frequency",
      availability: {
        status: "unavailable",
        reason: "iOS does not report core frequencies to the host",
      },
    },
    { capability: "logs", availability: { status: "available" } },
  ],
};

const deviceAction = vi.mocked(commands.deviceAction);

beforeEach(() => {
  vi.clearAllMocks();
  useAppStore.getState().reset();
});

/** Seeds the store; pass `null` for "not arrived yet". */
function seed(info: DeviceInfo | null = INFO, caps: CapabilitySet | null = CAPS) {
  act(() => {
    const s = useAppStore.getState();
    s.upsertDevice(IPHONE);
    s.patchDevice(IPHONE.id, { info: info ?? undefined, capabilities: caps ?? undefined });
  });
}

test("header and grouped fields render for the active device", () => {
  seed();
  render(<DeviceScreen />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("iPhone 16 Pro");
  expect(screen.getByText("fake:iphone")).toHaveClass("mono");
  expect(within(screen.getByRole("banner")).getByText("iOS")).toBeInTheDocument();

  const identity = within(screen.getByRole("region", { name: "Identity" }));
  expect(identity.getByText("iPhone17,1")).toBeInTheDocument();
  expect(identity.getByText("F2LXFAKE0001")).toBeInTheDocument();
  expect(identity.getByText("00008140-000A2C3E0E80001E")).toBeInTheDocument();

  const hardware = within(screen.getByRole("region", { name: "Hardware" }));
  expect(hardware.getByText("A18 Pro")).toBeInTheDocument();
  expect(hardware.getByText("6 (2 P + 4 E)")).toBeInTheDocument();
  expect(hardware.getByText("8.0 GB")).toBeInTheDocument();
  expect(hardware.getByText("3 582 mAh")).toBeInTheDocument();

  const state = within(screen.getByRole("region", { name: "State" }));
  expect(state.getByText("3 d 1 h 8 min")).toBeInTheDocument();
  expect(state.getByText("Yes")).toBeInTheDocument();

  const conn = within(screen.getByRole("region", { name: "Connection" }));
  expect(conn.getByText("USB")).toBeInTheDocument();
  expect(conn.getByText("userspace tunnel up")).toBeInTheDocument();
  expect(conn.getByText("12 s ago")).toBeInTheDocument();
  expect(conn.getByText("statasmon-device-fake 0.1.0")).toBeInTheDocument();
});

test("every missing field is a dash with the reason", () => {
  seed();
  render(<DeviceScreen />);
  const software = within(screen.getByRole("region", { name: "Software" }));
  const dashes = software.getAllByRole("img", { name: "Unavailable: Not reported by the device" });
  // security_patch and api_level are null on iOS.
  expect(dashes).toHaveLength(2);
  expect(dashes[0]).toHaveAttribute("title", "Not reported by the device");
  const hardware = within(screen.getByRole("region", { name: "Hardware" }));
  expect(hardware.getAllByRole("img", { name: /Unavailable/ })).toHaveLength(1); // cpu_max_mhz
  const state = within(screen.getByRole("region", { name: "State" }));
  expect(state.getAllByRole("img", { name: /Unavailable/ })).toHaveLength(3);
});

test("without info the sheet is all dashes and says it is waiting", () => {
  seed(null, null);
  render(<DeviceScreen />);
  expect(screen.getByText("Waiting for device information…")).toBeInTheDocument();
  const identity = within(screen.getByRole("region", { name: "Identity" }));
  expect(identity.getAllByRole("img", { name: /Unavailable/ })).toHaveLength(8);
  // The transport is known from the summary even before the info arrives.
  expect(within(screen.getByRole("region", { name: "Connection" })).getByText("USB")).toBeVisible();
  expect(screen.getByText("Waiting for the capability list…")).toBeInTheDocument();
});

test("capabilities list every entry with a pill or the honest reason", () => {
  seed();
  render(<DeviceScreen />);
  const list = screen.getByRole("list", { name: "Capability list" });
  expect(list).toHaveAttribute("data-count", "3");
  const items = within(list).getAllByRole("listitem");
  expect(items).toHaveLength(3);
  expect(items[0]).toHaveAttribute("data-available", "true");
  expect(within(items[0]).getByText("CPU per core")).toBeInTheDocument();
  expect(within(items[0]).getByText("Available")).toBeInTheDocument();
  expect(items[1]).toHaveAttribute("data-available", "false");
  expect(
    within(items[1]).getByText("iOS does not report core frequencies to the host"),
  ).toBeInTheDocument();
  expect(within(items[1]).queryByText("Available")).toBeNull();
  expect(screen.getByRole("region", { name: "Capabilities" })).toHaveTextContent(
    "2 of 3 available",
  );
});

test("raw keys are sorted, collapsible and copied as JSON", async () => {
  const writeText = vi.fn(async (_text: string) => undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  seed();
  render(<DeviceScreen />);
  expect(screen.queryByRole("table", { name: "Raw keys" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Show keys" }));
  const table = screen.getByRole("table", { name: "Raw keys" });
  const keys = within(table)
    .getAllByRole("rowheader")
    .map((th) => th.textContent);
  expect(keys).toEqual(["ActivationState", "ChipID", "HardwareModel", "ProductType"]);
  expect(within(table).getByText("D93AP")).toBeInTheDocument();

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Copy as JSON" }));
  });
  expect(writeText).toHaveBeenCalledWith(rawJson(INFO.raw));
  expect(JSON.parse(writeText.mock.calls[0][0])).toEqual(INFO.raw);
  expect(screen.getByRole("status")).toHaveTextContent("Copied");

  fireEvent.click(screen.getByRole("button", { name: "Hide keys" }));
  expect(screen.queryByRole("table", { name: "Raw keys" })).toBeNull();
});

test("Refresh asks the core for fresh info and reports the outcome", async () => {
  seed();
  render(<DeviceScreen />);
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  });
  expect(deviceAction).toHaveBeenCalledWith("fake:iphone", { action: "refresh_info" });
  expect(screen.getByRole("status")).toHaveTextContent("Refreshed");

  deviceAction.mockRejectedValueOnce({ code: "device_lost", message: "The device went away" });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  });
  expect(screen.getByRole("status")).toHaveTextContent("The device went away");
});

test("an explicit deviceId wins over the active device; none selected is stated", () => {
  render(<DeviceScreen />);
  expect(screen.getByText("No device selected")).toBeInTheDocument();
  seed();
  act(() => {
    useAppStore.getState().upsertDevice({ ...IPHONE, id: "fake:ipad", model_name: "iPad" });
  });
  render(<DeviceScreen deviceId="fake:ipad" />);
  expect(screen.getByRole("heading", { level: 1, name: "iPad" })).toBeInTheDocument();
});

test("model helpers: labels, ages, uptime, sorted raw, core line", () => {
  expect(capabilityLabel("wifi_details")).toBe("Wi-Fi details");
  expect(capabilityLabel("something_new")).toBe("Something new");
  expect(formatAge(10_000, 10_000)).toBe("just now");
  expect(formatAge(70_000, 10_000)).toBe("1 min ago");
  expect(formatAge(2 * 3_600_000 + 5 * 60_000, 0)).toBe("2 h 5 min ago");
  expect(formatAge(3 * 86_400_000, 0)).toBe("3 d ago");
  expect(formatUptime(59)).toBe("59 s");
  expect(formatUptime(3_661)).toBe("1 h 1 min");
  expect(sortedRaw({ b: "1", A: "2", c: "3" }).map(([k]) => k)).toEqual(["A", "b", "c"]);
  expect(sortedRaw(undefined)).toEqual([]);
  const rows = hardwareRows({
    ...INFO,
    hardware: { ...INFO.hardware, performance_cores: null, efficiency_cores: null, abis: [] },
  });
  expect(rows.find((r) => r.label === "CPU cores")?.value).toBe("6");
  expect(rows.find((r) => r.label === "ABIs")?.value).toBeNull();
});
