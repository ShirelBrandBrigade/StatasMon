import { expect, test } from "@playwright/test";

/**
 * Network screen on the browser replay of fixtures/fake/session-3dev-30s.jsonl,
 * reached through the sidebar with the fake iPhone active.
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#overview");
  const devices = page.getByRole("list", { name: "Devices" });
  await expect(devices.getByRole("button", { pressed: true })).toHaveCount(1, { timeout: 15_000 });
  await devices.getByRole("button", { name: /iPhone/ }).click();
  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("link", { name: "Network" })
    .click();
  await expect(page.getByTestId("network-screen")).toBeVisible();
});

test("throughput, packets, interface and per-process traffic render live values", async ({
  page,
}) => {
  await expect(page.getByTestId("net-down")).toContainText(/\d/, { timeout: 15_000 });
  await expect(page.getByTestId("net-down")).toContainText(/pkt\/s/);
  await expect(page.getByTestId("net-throughput")).toContainText(/down · .* up this session/);
  await expect(page.getByTestId("net-throughput").locator("canvas")).toHaveCount(2);

  const kv = page.getByTestId("net-interface-kv");
  await expect(kv).toContainText(/Connections\s*\d+/);
  await expect(kv).toContainText(/Latency\s*\d+(\.\d+)? ms/);
  await expect(
    kv.getByRole("img", { name: /Unavailable: iOS does not expose Wi-Fi signal details/ }).first(),
  ).toBeVisible();

  const rows = page.getByTestId("net-proc-row");
  await expect(rows.first()).toBeVisible({ timeout: 15_000 });
  await expect(rows.first()).toContainText("FieldKit");
  await expect(rows.first()).toContainText(/b\/s/);
});

test("packet capture starts and stops through device_action", async ({ page }) => {
  const input = page.getByLabel("Save to");
  await expect(input).toHaveValue(
    /^~\/Downloads\/StatasMon\/[a-z0-9-]+-\d{4}-\d{2}-\d{2}-\d{6}\.pcap$/,
  );
  await expect(page.getByTestId("pcap-state")).toContainText("Idle");
  await page.getByRole("button", { name: "Start capture" }).click();
  await expect(page.getByTestId("pcap-state")).toContainText(/Capturing since \d{2}:\d{2}:\d{2}/);
  await expect(input).toBeDisabled();
  await page.getByRole("button", { name: "Stop capture" }).click();
  await expect(page.getByTestId("pcap-state")).toContainText("Idle");
  await expect(page.getByRole("button", { name: "Start capture" })).toBeEnabled();
});

test("switching to the Android tablet shows its Wi-Fi details and the capture reason", async ({
  page,
}) => {
  await page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { name: /Galaxy/ })
    .click();
  const kv = page.getByTestId("net-interface-kv");
  await expect(kv).toContainText(/Signal \(RSSI\)\s*−\d+(\.\d+)? dBm/, { timeout: 15_000 });
  await expect(kv).toContainText(/Link speed\s*\d+ Mb\/s/);
  await expect(page.getByTestId("net-capture")).toContainText(/root/i);
  await expect(page.getByRole("button", { name: "Start capture" })).toBeDisabled();
});
