import { expect, test } from "@playwright/test";

/**
 * Performance screen on the browser replay of fixtures/fake/session-3dev-30s.jsonl,
 * reached through the sidebar with the fake iPhone active (6 cores, 2P + 4E).
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#overview");
  const devices = page.getByRole("list", { name: "Devices" });
  await expect(devices.getByRole("button", { pressed: true })).toHaveCount(1, { timeout: 15_000 });
  await devices.getByRole("button", { name: /iPhone/ }).click();
  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("link", { name: "Performance" })
    .click();
  await expect(page.getByTestId("performance-screen")).toBeVisible();
});

test("renders the four cards with live values and six per-core rows", async ({ page }) => {
  const screen = page.getByTestId("performance-screen");
  for (const id of ["perf-cpu", "perf-memory", "perf-thermal", "perf-battery"]) {
    await expect(screen.getByTestId(id), id).toBeVisible();
  }
  await expect(page.getByTestId("core-row")).toHaveCount(6, { timeout: 15_000 });
  await expect(page.getByTestId("core-row").first()).toContainText(/^P0/);
  await expect(page.getByTestId("core-row").first()).toContainText(/\d+%/);
  await expect(page.getByTestId("cpu-load")).toContainText(/load 1m \d+\.\d{2} · 5m \d+\.\d{2}/);
  await expect(page.getByTestId("mem-pressure")).toContainText(/Normal|Warning|Urgent|Critical/);
  await expect(page.getByTestId("perf-memory")).toContainText(/\d+\.\d GB used of \d+\.\d GB/);
  await expect(page.getByTestId("perf-battery")).toContainText(/\d\.\d+ V/);
  await expect(page.getByTestId("bat-kv")).toContainText(/\d+ mAh/);
  await expect(page.getByTestId("bat-state")).toContainText(/charging|discharging|full/);
});

test("draws every history chart on the global range and switches range without errors", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const screen = page.getByTestId("performance-screen");
  // CPU, memory, thermal state, temperatures, battery percent, voltage, current, temperature.
  await expect(screen.locator("canvas")).toHaveCount(8, { timeout: 15_000 });
  await page.getByRole("radio", { name: "Session" }).check();
  await expect(page.getByRole("radio", { name: "Session" })).toBeChecked();
  await page.waitForTimeout(1_500);
  await expect(screen.locator("canvas")).toHaveCount(8);
  expect(errors).toEqual([]);
});

test("the frequency column dashes with the iOS reason and the thermal event appears", async ({
  page,
}) => {
  const row = page.getByTestId("core-row").first();
  await expect(
    row.getByRole("img", { name: /Unavailable: iOS does not report core frequencies/ }),
  ).toBeVisible({ timeout: 15_000 });
  // The "warm" scenario raises the thermal state about 20 s into the replay.
  await expect(page.getByTestId("thermal-events")).toContainText(/Nominal → Fair/, {
    timeout: 30_000,
  });
  await expect(page.getByTestId("thermal-state")).toContainText("Fair");
});
