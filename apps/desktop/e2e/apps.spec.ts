import { expect, test } from "@playwright/test";

/**
 * Apps & processes (docs/PLAN.md 6.3) on the browser replay of
 * fixtures/fake/session-3dev-30s.jsonl: the iPhone's process table with
 * FieldKit (com.example.fieldkit) in the foreground.
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#apps");
  const devices = page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { pressed: true });
  await expect(devices).toHaveCount(1, { timeout: 15_000 });
});

/** Parses "434.5 MB" / "1.2 GB" cells into bytes for ordering checks. */
function parseBytes(text: string): number {
  const m = /([\d.]+)\s*([KMGT]?B)/.exec(text);
  if (!m) return Number.NaN;
  const unit = { B: 1, KB: 1024, MB: 1024 ** 2, GB: 1024 ** 3, TB: 1024 ** 4 }[m[2]] ?? 1;
  return Number.parseFloat(m[1]) * unit;
}

test("the table streams, sorts by memory, and FieldKit is highlighted", async ({ page }) => {
  const table = page.getByRole("table", { name: "Processes" });
  await expect(table).toBeVisible({ timeout: 15_000 });
  await expect(table).toHaveAttribute("data-count", /^[1-9]\d*$/);
  await expect(page.getByRole("columnheader", { name: /CPU/ })).toHaveAttribute(
    "aria-sort",
    "descending",
  );
  await expect(table.locator(".pt-row.is-me")).toContainText("FieldKit");

  await page.getByRole("button", { name: "Memory" }).click();
  await expect(page.getByRole("columnheader", { name: /Memory/ })).toHaveAttribute(
    "aria-sort",
    "descending",
  );
  const memCells = table.locator("tbody tr td:nth-child(5)");
  const first = parseBytes((await memCells.nth(0).textContent()) ?? "");
  const second = parseBytes((await memCells.nth(1).textContent()) ?? "");
  expect(first).toBeGreaterThanOrEqual(second);

  // Search narrows the rows; pin lifts a row to the top.
  await page.getByRole("searchbox", { name: "Search processes" }).fill("com.apple");
  await expect(table.locator("tbody tr").first()).toContainText(/com\.apple/);
  await page.getByRole("searchbox", { name: "Search processes" }).fill("");
  await page.getByRole("button", { name: "Pin Safari" }).click();
  await expect(table.locator("tbody tr").first()).toContainText("Safari");
  await expect(page.getByRole("button", { name: "Unpin Safari" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

test("opening FieldKit shows its charts, log and actions", async ({ page }) => {
  const table = page.getByRole("table", { name: "Processes" });
  await expect(table.locator(".pt-row.is-me")).toBeVisible({ timeout: 15_000 });
  await table.locator(".pt-row.is-me").click();
  const detail = page.getByRole("region", { name: "FieldKit detail" });
  await expect(detail).toBeVisible();
  await expect(detail).toContainText("com.example.fieldkit · pid");
  await expect(detail.locator("canvas")).toHaveCount(3, { timeout: 10_000 });
  await expect(detail.getByTestId("history-note")).toContainText(/[2-9]\d* of 900 points/, {
    timeout: 10_000,
  });
  const log = detail.getByRole("listbox", { name: "Log lines of FieldKit" });
  await expect(log).toHaveAttribute("data-count", /^[1-9]\d*$/, { timeout: 10_000 });
  await expect(log.getByRole("option").first()).toContainText(/\d\d:\d\d:\d\d/);

  await detail.getByRole("button", { name: "Export this app's log" }).click();
  const dialog = page.getByRole("dialog", { name: "Export" });
  await expect(dialog.getByRole("radio", { name: "One app" })).toBeChecked();
  await page.keyboard.press("Escape");

  await detail.getByRole("button", { name: "Focus in log" }).click();
  await expect(page).toHaveURL(/#logs$/);
  const fieldkit = page
    .getByRole("tablist", { name: "Apps" })
    .getByRole("tab", { name: /^FieldKit/ });
  await expect(fieldkit).toHaveAttribute("aria-selected", "true");
});
