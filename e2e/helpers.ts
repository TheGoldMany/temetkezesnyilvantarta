import { expect, type Page } from "@playwright/test";

export async function login(page: Page) {
  await page.goto("/");
  const user = page.locator("input[name=username]");
  await page.locator('input[name=username], nav[aria-label="Fő menü"]').first().waitFor({ timeout: 30_000 });
  if (await user.isVisible()) {
    await user.fill("admin");
    await page.locator("input[name=password]").fill("admin-jelszo");
    await page.getByRole("button", { name: "Belépés" }).click();
  }
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
}

export async function nav(page: Page, label: string) {
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: label, exact: true }).click();
}
