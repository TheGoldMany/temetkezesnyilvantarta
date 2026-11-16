import { expect, test } from "@playwright/test";
import { login } from "./helpers";

test("új verzió: értesítés jobb alul, frissítés gombbal", async ({ page }) => {
  await login(page);
  await expect(page.getByTestId("update-notice")).toHaveCount(0);
  // a new version is deployed while the program is open
  await page.route("**/version.json*", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ build: "uj-verzio" }) }));
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  const notice = page.getByTestId("update-notice");
  await expect(notice).toContainText("Új verzió érhető el");
  const box = await notice.boundingBox();
  const vp = page.viewportSize()!;
  expect(box!.x + box!.width).toBeGreaterThan(vp.width - 40);
  expect(box!.y + box!.height).toBeGreaterThan(vp.height - 40);
  // closed: not offered again while the page stays open
  await notice.getByRole("button", { name: "Bezárás" }).click();
  await expect(notice).toHaveCount(0);
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.waitForTimeout(500);
  await expect(page.getByTestId("update-notice")).toHaveCount(0);
  // after reloading, the running version is the current one again
  await page.unroute("**/version.json*");
  await page.reload();
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.waitForTimeout(500);
  await expect(page.getByTestId("update-notice")).toHaveCount(0);
});
