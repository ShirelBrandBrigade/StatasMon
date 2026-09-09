import { expect, test } from "@playwright/test";

/**
 * Device screen (Phase 6.5) on the browser replay of
 * fixtures/fake/session-3dev-30s.jsonl: the fake iPhone's grouped fields,
 * then the fake iPad's capability reasons after switching device, and the
 * raw keys with "Copy as JSON".
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#overview");
  const devices = page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { pressed: true });
  await expect(devices).toHaveCount(1, { timeout: 15_000 });
  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("link", { name: "Device", exact: true })
    .click();
  await expect(page.getByTestId("device-screen")).toBeVisible();
});

test("shows the fake iPhone's identity, software, hardware, state and connection groups", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await expect(page.getByRole("heading", { level: 1 })).toHaveText("iPhone 16 Pro");
  await expect(page.getByTestId("device-screen")).toHaveAttribute("data-device-id", "fake:iphone");

  const identity = page.getByRole("region", { name: "Identity" });
  await expect(identity).toContainText("iPhone17,1", { timeout: 15_000 });
  await expect(identity).toContainText("F2LXFAKE0001");
  await expect(identity).toContainText("00008140-000A2C3E0E80001E");

  const software = page.getByRole("region", { name: "Software" });
  await expect(software).toContainText("iOS");
  await expect(software).toContainText("25A354");
  // Not reported on iOS: security patch and API level are dashes with the reason.
  await expect(
    software.getByRole("img", { name: "Unavailable: Not reported by the device" }),
  ).toHaveCount(2);

  const hardware = page.getByRole("region", { name: "Hardware" });
  await expect(hardware).toContainText("A18 Pro");
  await expect(hardware).toContainText("6 (2 P + 4 E)");
  await expect(hardware).toContainText("8.0 GB");
  await expect(hardware).toContainText("3 582 mAh");

  const state = page.getByRole("region", { name: "State" });
  await expect(state).toContainText("Europe/Paris");
  await expect(state.getByText("Yes")).toBeVisible();

  const connection = page.getByRole("region", { name: "Connection" });
  await expect(connection).toContainText("tier 2");
  await expect(connection).toContainText("userspace tunnel up");
  await expect(connection).toContainText(/ago|just now/);
  await expect(connection).toContainText("statasmon-device-fake 0.1.0");

  const caps = page.getByRole("list", { name: "Capability list" });
  await expect(caps).toHaveAttribute("data-count", "27");
  await expect(caps.locator('[data-capability="cpu_frequency"]')).toContainText(
    "iOS does not report core frequencies to the host",
  );
  expect(errors).toEqual([]);
});

test("switching to the fake iPad shows its capability reasons and raw keys copy as JSON", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { name: /Elia's iPad/ })
    .click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("iPad (10th generation)");
  await expect(page.getByTestId("device-screen")).toHaveAttribute("data-device-id", "fake:ipad");

  const caps = page.getByRole("list", { name: "Capability list" });
  await expect(caps).toHaveAttribute("data-count", "27", { timeout: 15_000 });
  await expect(caps.locator('[data-available="false"]')).toHaveCount(15);
  await expect(caps.locator('[data-available="true"]')).toHaveCount(12);
  await expect(page.getByRole("region", { name: "Capabilities" })).toContainText(
    "12 of 27 available",
  );
  const cpu = caps.locator('[data-capability="cpu_total"]');
  await expect(cpu).toHaveAttribute("data-available", "false");
  await expect(cpu).toContainText("Developer Mode is off on this iPad");
  await expect(caps.locator('[data-capability="logs"]')).toContainText("Available");

  const stateGroup = page.getByRole("region", { name: "State" });
  await expect(stateGroup.getByText("No")).toBeVisible(); // Developer Mode off
  await expect(page.getByRole("region", { name: "Connection" })).toContainText(
    "not started (Developer Mode off)",
  );

  // Raw keys: collapsed by default, sorted when shown, copied as JSON.
  const raw = page.getByRole("region", { name: "Raw" });
  await expect(raw).toContainText("8 keys");
  await expect(page.getByRole("table", { name: "Raw keys" })).toHaveCount(0);
  await raw.getByRole("button", { name: "Show keys" }).click();
  const table = page.getByRole("table", { name: "Raw keys" });
  await expect(table).toBeVisible();
  const keys = await table.getByRole("rowheader").allTextContents();
  expect(keys).toEqual([...keys].sort((a, b) => a.localeCompare(b, "en", { sensitivity: "base" })));
  expect(keys).toContain("HardwareModel");
  await expect(table.getByRole("row", { name: /HardwareModel/ })).toContainText("J271AP");

  await raw.getByRole("button", { name: "Copy as JSON" }).click();
  await expect(page.getByRole("status")).toHaveText("Copied");
  const clipboard = await page.evaluate(() => navigator.clipboard.readText());
  const parsed = JSON.parse(clipboard) as Record<string, string>;
  expect(parsed.HardwareModel).toBe("J271AP");
  expect(parsed.ProductType).toBe("iPad13,18");
  expect(Object.keys(parsed)).toHaveLength(8);
});

test("Refresh asks the fake device for fresh information", async ({ page }) => {
  await page.getByRole("button", { name: "Refresh" }).click();
  // The replay answers device_action with null (no error), so the sheet reports success.
  await expect(page.getByRole("status")).toHaveText("Refreshed");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("iPhone 16 Pro");
});
