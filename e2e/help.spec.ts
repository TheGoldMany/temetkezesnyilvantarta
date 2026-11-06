import { expect, test } from "@playwright/test";
import { login } from "./helpers";

// The guided tour visits every module and ends on the Súgó page; the help
// page explains how to create a parcel.
test("bemutató végig, súgó keresés", async ({ page }) => {
  // the strict Content Security Policy must not block anything the app does
  const violations: string[] = [];
  page.on("console", (m) => {
    if (/Content Security Policy|Refused to/i.test(m.text())) violations.push(m.text());
  });
  const res = await page.goto("/");
  expect(res?.headers()["content-security-policy"]).toContain("frame-ancestors 'none'");
  await login(page);
  await page.getByRole("button", { name: "Súgó" }).click();
  await page.getByRole("button", { name: "Bemutató indítása" }).click();
  const tour = page.getByTestId("tour");
  await expect(tour).toContainText("Üdvözöljük");
  let steps = 0;
  while (await tour.getByRole("button", { name: "Tovább" }).isVisible()) {
    const title = await tour.locator("h2").innerText();
    await tour.getByRole("button", { name: "Tovább" }).click();
    await expect(tour.locator("h2")).not.toHaveText(title);
    steps += 1;
    expect(steps).toBeLessThan(30);
  }
  await expect(tour).toContainText("Kész is!");
  await tour.getByRole("button", { name: "Befejezés" }).click();
  await expect(tour).toHaveCount(0);
  expect(steps).toBeGreaterThan(12);

  await page.getByPlaceholder(/Keresés a súgóban/).fill("parcella");
  await expect(page.getByText("Új parcella létrehozása")).toBeVisible();
  await expect(page.getByText(/Parcellák és sorok létrehozása/).first()).toBeVisible();
  expect(violations).toEqual([]);
});
