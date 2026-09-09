import { expect, test } from "@playwright/test";

/**
 * Sessions screen and Export dialog on the browser replay (Phase 5 UI). The
 * replay answers `list_sessions` with one live session per replayed device
 * plus two synthetic stored sessions from yesterday, and `export_data` with
 * three progress messages before the result.
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#overview");
  const devices = page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { pressed: true });
  await expect(devices).toHaveCount(1, { timeout: 15_000 });
});

test("Sessions lists three live and two stored sessions, opens one read-only, compares two", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("link", { name: "Sessions" })
    .click();
  await expect(page.getByTestId("sessions-screen")).toBeVisible();
  const rows = page.getByTestId("session-row");
  await expect(rows).toHaveCount(5, { timeout: 15_000 });
  await expect(page.locator('[data-testid="session-row"][data-live="true"]')).toHaveCount(3);
  await expect(page.locator('[data-testid="session-row"][data-live="false"]')).toHaveCount(2);
  // Newest first: the live ones (today) before yesterday's stored ones.
  await expect(rows.first()).toHaveAttribute("data-live", "true");
  await expect(rows.last()).toHaveAttribute("data-live", "false");
  await expect(rows.last()).toContainText(/samples/);
  await expect(page.getByTestId("store-stats")).toContainText(/\d+ sessions · [\d.]+ [KMG]?B/);

  // Open a stored session: read-only detail with device sheet, timeline and a chart.
  const stored = page.locator('[data-testid="session-row"][data-live="false"]').first();
  await stored.getByRole("button", { name: "Open" }).click();
  const detail = page.getByTestId("session-detail");
  await expect(detail).toBeVisible();
  await expect(detail.getByText("Read-only")).toBeVisible();
  await expect(
    detail.getByRole("list", { name: "Marks and events" }).getByRole("listitem"),
  ).toHaveCount(6);
  const metric = detail.getByRole("combobox", { name: "Metric" });
  await expect(metric).toHaveValue("cpu.total");
  await expect(detail.locator("canvas")).toHaveCount(1, { timeout: 10_000 });

  await metric.selectOption("mem.used");
  await expect(metric).toHaveValue("mem.used");
  await expect(detail.locator("canvas")).toHaveCount(1);

  // Compare with the other stored session on the same metric.
  await detail.getByRole("button", { name: "Compare" }).click();
  const second = detail.getByRole("combobox", { name: "Second session" });
  const otherId = await page
    .locator('[data-testid="session-row"][data-live="false"]')
    .nth(1)
    .getAttribute("data-session-id");
  await second.selectOption({ index: 1 });
  await expect(second).not.toHaveValue("");
  const compare = page.getByTestId("compare-chart");
  await expect(compare).toBeVisible();
  await expect(compare).toContainText(/A · /);
  await expect(compare).toContainText(/B · /);
  await expect(compare.locator("canvas")).toHaveCount(1);
  expect(otherId).not.toBeNull();

  // Delete the other stored session (confirmed inline) and watch the list shrink.
  const victim = page.locator(`[data-session-id="${otherId}"]`);
  await victim.getByRole("button", { name: "Delete" }).click();
  await victim.getByRole("button", { name: "Yes, delete" }).click();
  await expect(rows).toHaveCount(4);
  expect(errors).toEqual([]);
});

test("Export from the TopBar runs to 100 % and shows the result line", async ({ page }) => {
  await page.getByRole("button", { name: "Export", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Export" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("radio", { name: "Whole session" })).toBeChecked();
  await dialog.getByRole("radio", { name: "CSV" }).check();
  const path = dialog.getByLabel("Destination");
  await expect(path).toHaveValue(/\.csv$/);
  await dialog.getByRole("button", { name: "Export", exact: true }).click();
  await expect(dialog.getByRole("progressbar", { name: "Export progress" })).toBeVisible();
  await expect(dialog.getByTestId("export-progress")).toContainText("100 %", { timeout: 5_000 });
  const result = dialog.getByTestId("export-result");
  await expect(result).toContainText("Wrote 12 345 B · 100 entries · ~/Downloads/StatasMon/");
  await expect(result.getByRole("button", { name: "Reveal" })).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // ⌘/Ctrl+E opens it too.
  await page.keyboard.press("ControlOrMeta+e");
  await expect(page.getByRole("dialog", { name: "Export" })).toBeVisible();
});
