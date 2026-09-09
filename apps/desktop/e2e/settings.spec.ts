import { expect, test } from "@playwright/test";

/**
 * Settings screen (Phase 6.6) in browser replay mode: `get_settings` and
 * `set_settings` are answered from an in-memory object, so a saved edit
 * survives navigating away and back within the page but not a reload.
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#overview");
  const devices = page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { pressed: true });
  await expect(devices).toHaveCount(1, { timeout: 15_000 });
  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("link", { name: "Settings", exact: true })
    .click();
  await expect(page.getByTestId("settings-screen")).toHaveAttribute("data-loaded", "true");
});

test("loads the defaults, edits fast_ms, saves and keeps the value across navigation", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  const fast = page.getByLabel("Fast");
  await expect(fast).toHaveValue("1000");
  await expect(page.getByLabel("Medium")).toHaveValue("3000");
  await expect(page.getByLabel("Slow")).toHaveValue("15000");
  await expect(page.getByLabel("Heavy")).toHaveValue("0");
  await expect(page.getByLabel("Hours per session")).toHaveValue("4");
  await expect(page.getByLabel("Days to keep")).toHaveValue("30");
  await expect(page.getByLabel("adb path")).toHaveValue("");
  await expect(page.getByRole("region", { name: "Android" })).toContainText("auto-detected");
  await expect(page.getByRole("region", { name: "Storage" })).toContainText("(replay)");
  await expect(page.getByRole("region", { name: "About" })).toContainText("StatasMon");

  const save = page.getByRole("button", { name: "Save" });
  await expect(save).toBeDisabled();

  await fast.fill("500");
  await expect(page.getByTestId("settings-screen")).toHaveAttribute("data-dirty", "true");
  await expect(save).toBeEnabled();
  await save.click();
  await expect(page.getByRole("status")).toHaveText("Saved");
  await expect(page.getByTestId("settings-screen")).toHaveAttribute("data-dirty", "false");
  await expect(save).toBeDisabled();

  // Away and back: the replay core hands the stored value back.
  const nav = page.getByRole("navigation", { name: "Views" });
  await nav.getByRole("link", { name: "Overview", exact: true }).click();
  await expect(page.getByTestId("settings-screen")).toHaveCount(0);
  await nav.getByRole("link", { name: "Settings", exact: true }).click();
  await expect(page.getByTestId("settings-screen")).toHaveAttribute("data-loaded", "true");
  await expect(page.getByLabel("Fast")).toHaveValue("500");
  expect(errors).toEqual([]);
});

test("out-of-range values are flagged inline and never sent", async ({ page }) => {
  const fast = page.getByLabel("Fast");
  await fast.fill("10");
  await expect(fast).toHaveAttribute("aria-invalid", "true");
  await expect(page.getByText("Fast must be a whole number of at least 250 ms.")).toBeVisible();
  await page.getByLabel("Days to keep").fill("400");
  await expect(page.getByText("Days to keep must be from 1 to 365.")).toBeVisible();

  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Fix the highlighted fields first");
  await expect(page.getByTestId("settings-screen")).toHaveAttribute("data-dirty", "true");

  await page.getByRole("button", { name: "Revert" }).click();
  await expect(fast).toHaveValue("1000");
  await expect(page.getByLabel("Days to keep")).toHaveValue("30");
  await expect(page.getByRole("button", { name: "Save" })).toBeDisabled();
});

test("the update check is opt-in and saved as a flag", async ({ page }) => {
  const box = page.getByRole("checkbox", { name: /Check for updates on launch/ });
  await expect(box).not.toBeChecked();
  await box.check();
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved");
  await expect(box).toBeChecked();
});
