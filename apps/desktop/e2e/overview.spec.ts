import { expect, test } from "@playwright/test";

/**
 * Overview on the browser replay of fixtures/fake/session-3dev-30s.jsonl.
 * Three devices attach immediately; samples, cores, processes and logs
 * stream in; the "warm" scenario raises the thermal state after ~20 s.
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#overview");
  const devices = page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { pressed: true });
  await expect(devices).toHaveCount(1, { timeout: 15_000 });
});

test("lists three devices and shows the active headline", async ({ page }) => {
  const devices = page.getByRole("list", { name: "Devices" }).getByRole("button");
  await expect(devices).toHaveCount(3);
  await expect(page.getByRole("heading", { level: 1 })).toContainText(/iPhone|iPad|Galaxy/);
  await expect(page.getByRole("region", { name: "Recording" })).toContainText(
    /Recording|Paused|Idle/,
  );
});

test("every card shows live values, not only dashes", async ({ page }) => {
  for (const slot of ["cpu", "memory", "storage", "battery", "network", "thermal", "apps"]) {
    const card = page.locator(`[data-slot="${slot}"]`);
    await expect(card, slot).toBeVisible();
    await expect(card, slot).not.toContainText("Card in progress");
  }
  // Numbers arrive within a few ticks of replay.
  await expect(page.locator('[data-slot="cpu"]')).toContainText(/\d+%/, { timeout: 10_000 });
  await expect(page.locator('[data-slot="battery"]')).toContainText(/\d+\.\d+ V/);
  await expect(page.locator('[data-testid="net-down"]')).toContainText(/\d/);
  await expect(page.locator('[data-slot="network"] canvas')).toHaveCount(1, { timeout: 10_000 });
  await expect(page.locator('[data-slot="apps"]')).toContainText(/FieldKit/, { timeout: 10_000 });
});

test("switching device changes the headline and keeps the layout", async ({ page }) => {
  const devices = page.getByRole("list", { name: "Devices" }).getByRole("button");
  const before = await page.getByRole("heading", { level: 1 }).textContent();
  await devices.nth(1).click();
  await expect(devices.nth(1)).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("heading", { level: 1 })).not.toHaveText(before ?? "");
  await expect(page.locator("[data-slot]")).toHaveCount(8);
});

test("the log streams, and a level toggle filters rows", async ({ page }) => {
  const list = page.getByRole("listbox");
  await expect(list).toHaveAttribute("data-count", /^[1-9]\d*$/, { timeout: 15_000 });
  await expect(list).toHaveAttribute("data-follow", "following");
  const info = page
    .getByRole("toolbar", { name: "Log filters" })
    .getByRole("button", { name: "Info" });
  await expect(info).toHaveAttribute("aria-pressed", "true");
  const countBefore = Number(await list.getAttribute("data-count"));
  await info.click();
  await expect(info).toHaveAttribute("aria-pressed", "false");
  await expect
    .poll(async () => Number(await list.getAttribute("data-count")), { timeout: 5_000 })
    .toBeLessThan(countBefore + 50);
  const rows = list.getByRole("option");
  await expect(rows.first()).toBeVisible();
  await expect(rows.filter({ hasText: /\bInfo\b/ })).toHaveCount(0);
});

test("range segment and pause/resume update the store and the recording card", async ({ page }) => {
  await page.getByRole("radio", { name: "1 h" }).check();
  await expect(page.getByRole("radio", { name: "1 h" })).toBeChecked();
  const rec = page.getByRole("region", { name: "Recording" });
  await rec.getByRole("button", { name: /Pause/ }).click();
  await expect(rec).toContainText(/Paused/);
  await rec.getByRole("button", { name: /Resume/ }).click();
  await expect(rec).toContainText(/Recording/);
  await rec.getByRole("button", { name: /Mark/ }).click();
});

test("navigation renders the other views and the gallery", async ({ page }) => {
  await page.getByRole("navigation", { name: "Views" }).getByRole("link", { name: "Logs" }).click();
  await expect(page.getByTestId("logs-screen")).toBeVisible();
  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("link", { name: "Apps & processes" })
    .click();
  await expect(page.getByTestId("apps-screen")).toBeVisible();
  await page.goto("/#gallery");
  await expect(page.getByRole("heading", { name: /gallery/i })).toBeVisible();
});

test("no console errors during a 5 s live run", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    // React dev warnings on first mount are tracked in docs/KNOWN-ISSUES.md.
    if (m.type() === "error" && !/unique "key"|NaN.*paddingBottom/.test(m.text()))
      errors.push(m.text());
  });
  await page.reload();
  await page.waitForTimeout(5_000);
  expect(errors).toEqual([]);
});
