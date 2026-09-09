import { expect, test } from "@playwright/test";

/**
 * "Add a device" dialog and the Notices stack on the browser replay of
 * fixtures/fake/session-3dev-30s.jsonl. In replay `device_action` answers
 * null, so the dialog must stay open and show a neutral "No response" line.
 */
test.beforeEach(async ({ page }) => {
  await page.goto("/#overview");
  const devices = page
    .getByRole("list", { name: "Devices" })
    .getByRole("button", { pressed: true });
  await expect(devices).toHaveCount(1, { timeout: 15_000 });
});

test("the dialog opens from the sidebar, the pair form submits, Escape closes it", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.getByRole("button", { name: "Add a device" }).click();
  const dialog = page.getByRole("dialog", { name: "Add a device" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("Pair over Wi-Fi (Android 11+)")).toBeVisible();

  const pair = dialog.getByRole("form", { name: "Pair over Wi-Fi (Android 11+)" });
  await expect(pair.getByLabel("Host")).toBeFocused();
  await pair.getByLabel("Host").fill("192.168.1.20");
  await pair.getByLabel("Pairing port").fill("37001");
  await pair.getByLabel("Pairing code").fill("123456");
  await pair.getByRole("button", { name: "Pair" }).click();

  const outcome = pair.getByRole("status");
  await expect(outcome).toHaveText("No response");
  await expect(outcome).toHaveAttribute("data-outcome", "none");
  await expect(dialog).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("validation blocks a bad code and port without calling anything", async ({ page }) => {
  await page.getByRole("button", { name: "Add a device" }).click();
  const pair = page.getByRole("form", { name: "Pair over Wi-Fi (Android 11+)" });
  await pair.getByLabel("Host").fill("192.168.1.20");
  await pair.getByLabel("Pairing port").fill("99999");
  await pair.getByLabel("Pairing code").fill("12");
  await pair.getByRole("button", { name: "Pair" }).click();
  await expect(pair.getByText("Port must be a number from 1 to 65535.")).toBeVisible();
  await expect(pair.getByText("The pairing code is 6 digits.")).toBeVisible();
  await expect(pair.getByRole("status")).toHaveCount(0);
});

test("the Notices stack is empty on the fixture", async ({ page }) => {
  const notices = page.getByRole("list", { name: "Notices" });
  await expect(notices).toHaveAttribute("data-count", "0");
  await page.waitForTimeout(3_000);
  await expect(notices).toHaveAttribute("data-count", "0");
});
