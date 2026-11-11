import { expect, test } from "@playwright/test";
import { lastMail, login, nav } from "./helpers";

test("ingyenes bemutató: táblázat feltöltése a weboldalon, a platform-admin letölti", async ({ page, browser }) => {
  const town = `Próbarét ${Date.now() % 1000000}`;
  const email = `jegyzo-${Date.now()}@pelda.hu`;
  await page.goto("/bemutato");
  await expect(page.getByRole("heading", { name: /Hogyan festene az Ön temetője/ })).toBeVisible();
  await page.getByLabel("Település vagy temető neve").fill(town);
  await page.getByLabel("Az Ön neve").fill("Jegyző Judit");
  await page.getByLabel(/Beosztása/).fill("jegyző");
  await page.getByLabel("E-mail-cím").fill(email);
  await page.getByLabel(/A táblázat/).setInputFiles({ name: "temeto.csv", mimeType: "text/csv", buffer: Buffer.from("név;sírhely\nKovács János;A/1/1\n", "utf-8") });
  // consent is required
  await page.getByRole("button", { name: "Feltöltés és bemutató kérése" }).click();
  await expect(page.getByRole("status")).toContainText("adatkezelési");
  await page.getByRole("checkbox").check();
  await page.getByRole("button", { name: "Feltöltés és bemutató kérése" }).click();
  await expect(page.getByRole("status")).toContainText("Köszönjük");
  expect((await lastMail(page, email, "demo_request_ack")).body).toContain(town);

  const admin = await (await browser.newContext()).newPage();
  await login(admin);
  await nav(admin, "Ügyfelek");
  const row = admin.getByTestId("demo-requests").getByRole("listitem").filter({ hasText: town });
  await expect(row).toContainText("Jegyző Judit");
  await expect(row).toContainText("temeto.csv");
  const download = admin.waitForEvent("download");
  await row.getByRole("button", { name: "Fájl letöltése" }).click();
  expect((await download).suggestedFilename()).toBe("temeto.csv");
  await row.getByRole("button", { name: "Kész" }).click();
  await expect(row).toContainText("kész");
});
