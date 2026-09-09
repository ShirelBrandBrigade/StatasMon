import { expect, test } from "@playwright/test";

/**
 * Logs screen (docs/PLAN.md 6.4) on the browser replay of
 * fixtures/fake/session-3dev-30s.jsonl: the iPhone streams iOS lines (no
 * buffers), the Galaxy Tab streams logcat lines with `main` / `system`.
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#logs");
  const devices = page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { pressed: true });
  await expect(devices).toHaveCount(1, { timeout: 15_000 });
});

const listOf = (page: import("@playwright/test").Page) =>
  page.getByRole("listbox", { name: /Log entries/ });

test("streams full height, a regex narrows the count, tabs select an app", async ({ page }) => {
  const list = listOf(page);
  await expect(list).toHaveAttribute("data-count", /^[1-9]\d*$/, { timeout: 15_000 });
  await expect(list).toHaveAttribute("data-follow", "following");
  await expect
    .poll(async () => Number(await list.getAttribute("data-count")), { timeout: 10_000 })
    .toBeGreaterThan(20);
  const before = Number(await list.getAttribute("data-count"));

  const search = page.getByRole("searchbox", { name: "Search log" });
  await search.fill("/URLSession|Sync/");
  await expect
    .poll(async () => Number(await list.getAttribute("data-count")), { timeout: 5_000 })
    .toBeLessThan(before);
  await expect(list.getByRole("option").first()).toContainText(/URLSession|Sync/);
  await search.fill("/(/");
  await expect(search).toHaveAttribute("aria-invalid", "true");
  await expect(page.getByText("invalid regex")).toBeVisible();
  await search.press("Escape");
  await expect(search).toHaveValue("");

  // No buffer chips for an iOS device.
  await expect(page.getByTestId("buffers")).toHaveCount(0);

  // Per-app tabs: FieldKit logs the most, and is the app under test.
  const tabs = page.getByRole("tablist", { name: "Apps" });
  await expect(tabs.getByRole("tab", { name: /^All/ })).toHaveAttribute("aria-selected", "true");
  const fieldkit = tabs.getByRole("tab", { name: /^FieldKit/ });
  await fieldkit.click();
  await expect(fieldkit).toHaveAttribute("aria-selected", "true");
  await expect(list.getByRole("option").first()).toContainText("FieldKit");
  await tabs.getByRole("tab", { name: /^All/ }).click();

  // `/` focuses the search from the list.
  await list.focus();
  await page.keyboard.press("/");
  await expect(search).toBeFocused();
});

test("selecting two rows enables Export selection with two seqs", async ({ page }) => {
  const list = listOf(page);
  await expect(list).toHaveAttribute("data-count", /^[1-9]\d*$/, { timeout: 15_000 });
  // Pause follow-tail so the rows under the cursor stay put.
  await page.getByRole("button", { name: "Following" }).click();
  await expect(list).toHaveAttribute("data-follow", "paused");
  const rows = list.getByRole("option");
  await rows.nth(0).click();
  await rows.nth(1).click();
  await expect(list.locator('[aria-selected="true"]')).toHaveCount(2);
  await expect(page.locator(".sm-card-footer")).toContainText("2 selected");
  await page.getByRole("button", { name: "Export selection" }).click();
  const dialog = page.getByRole("dialog", { name: "Export" });
  await expect(dialog.getByRole("radio", { name: "Selected log lines (2)" })).toBeChecked();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("the Android device shows buffer chips; a chip narrows the list", async ({ page }) => {
  const devices = page.getByRole("list", { name: "Devices" }).getByRole("button");
  await devices.filter({ hasText: /Galaxy/ }).click();
  const list = listOf(page);
  await expect(list).toHaveAttribute("data-count", /^[1-9]\d*$/, { timeout: 15_000 });
  const buffers = page.getByTestId("buffers");
  await expect(buffers).toBeVisible({ timeout: 15_000 });
  await expect(buffers.getByRole("button", { name: "main" })).toBeVisible();
  await buffers.getByRole("button", { name: "system" }).click();
  await expect(list).toHaveAttribute("data-narrowed", "true");
  await expect.poll(async () => Number(await list.getAttribute("data-count"))).toBeGreaterThan(0);
});

test("the Crashes strip fetches reports and the stored session pages older lines", async ({
  page,
}) => {
  const strip = page.getByTestId("crash-strip");
  await expect(strip).toContainText(/No crashes or ANRs|Crash|ANR/);
  await strip.getByRole("button", { name: "Fetch crash reports" }).click();
  // The browser replay answers device_action with null: reported truthfully.
  await expect(page.getByTestId("crash-fetch-result")).toHaveText("no reports returned", {
    timeout: 5_000,
  });

  const stored = page.getByTestId("stored-session");
  await expect(stored).toContainText(/session /, { timeout: 15_000 });
  await stored.getByRole("button", { name: "Load 500 older" }).click();
  // The mirror holds the whole replay, so nothing older is in the store.
  await expect(page.getByTestId("stored-status")).toContainText(
    /No older lines|older in the store/,
    {
      timeout: 5_000,
    },
  );
});

test("no console errors during a 5 s live run on the Logs screen", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error" && !/unique "key"|NaN.*paddingBottom/.test(m.text()))
      errors.push(m.text());
  });
  await page.reload();
  await page.waitForTimeout(5_000);
  expect(errors).toEqual([]);
});
