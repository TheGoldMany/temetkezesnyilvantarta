import { expect, test } from "@playwright/test";
import { lastMail, login, nav } from "./helpers";

// W12: annual report with capacity, e-mail expiry notice with delivery log, public grave search.
test("fenntartói beszámoló, e-mail értesítő, nyilvános sírkereső", async ({ page }) => {
  await login(page);

  // annual report of the current year (the sample data has burials in it)
  await nav(page, "Nyomtatás és riportok");
  await page.getByRole("tab", { name: "Riportok" }).click();
  await page.locator("select[name=annualYear]").selectOption(String(new Date().getFullYear()));
  const report = page.getByTestId("annual-report");
  await expect(report).toContainText("Temetések fajtánként");
  await expect(page.getByTestId("capacity").getByRole("row")).not.toHaveCount(1);

  // the holder of an expiring plot gets an e-mail address, then the notice goes by e-mail
  await nav(page, "Lejáratok");
  await page.getByLabel("Csak az értesítetlenek").check();
  const rows = page.locator("tbody tr");
  await expect(rows.first()).toBeVisible();
  const ident = (await rows.nth(0).locator("input[type=checkbox]").getAttribute("aria-label"))!;
  const holder = (await rows.nth(0).locator("td").nth(4).evaluate((el) => el.firstChild?.textContent ?? "")).trim();
  expect(holder).not.toBe("");
  // plot sheet → holder → edit
  await rows.nth(0).locator("td").nth(1).getByRole("button").click();
  await page.getByRole("button", { name: holder, exact: true }).first().click();
  await page.getByRole("button", { name: "Szerkesztés" }).click();
  const address = `jogosult-${Date.now()}@pelda.hu`;
  await page.getByRole("dialog").getByLabel("E-mail").fill(address);
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await nav(page, "Lejáratok");
  await page.getByLabel("Csak az értesítetlenek").check();
  await page.getByRole("checkbox", { name: ident, exact: true }).check();
  await page.getByRole("button", { name: "Értesítés e-mailben" }).click();
  await expect(page.getByRole("dialog").getByText("1 e-mail elküldve, az értesítés rögzítve.")).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Bezárás" }).last().click();
  const mail = await lastMail(page, address, "expiry_notice");
  expect(mail.subject).toContain("Temetési hely lejárata");
  await expect(page.getByTestId("email-log")).toContainText(address);

  // public grave search: off until published, then the embed page finds the dead
  await nav(page, "Beállítások");
  await page.getByRole("tab", { name: "Sírkereső" }).click();
  await page.getByLabel("Sírkereső közzététele").click();
  await expect(page.getByLabel("Sírkereső közzététele")).toBeChecked();
  await expect(page.getByLabel("Díjak megjelenítése a keresőben")).toBeChecked();
  const code = page.locator("textarea[name=embedCode]");
  await expect(code).toHaveValue(/<iframe src=/);
  const src = (await code.inputValue()).match(/src="([^"]+)"/)![1];
  const url = new URL(src);
  await page.goto(url.pathname + url.search);
  await page.getByLabel("Elhunyt neve").fill("Kovács");
  await page.getByRole("button", { name: "Keresés" }).click();
  await expect(page.getByRole("status")).toContainText("találat");
  await expect(page.getByRole("list", { name: "Találatok" }).getByRole("listitem").first()).toContainText("Kovács");
  // opening hours and the public map with the plot highlighted
  await expect(page.getByRole("list", { name: "Temetők" })).toContainText("Nyitvatartás:");
  await page.getByRole("list", { name: "Találatok" }).getByRole("button", { name: "Térképen" }).first().click();
  await expect(page.locator("#map rect.hl")).toHaveCount(1);
  // the fee list in force, gross
  const fees = page.locator("#feebox");
  await expect(fees.getByRole("heading", { name: "Díjak" })).toBeVisible();
  await expect(fees.getByRole("row").filter({ hasText: "Sírhely megváltása" }).first()).toContainText(/\d\sFt/);
  // the public endpoint gives no birth dates or personal data
  const res = await page.request.get(`/api/public/search${url.search}&q=Kov%C3%A1cs`);
  expect(res.headers()["access-control-allow-origin"]).toBe("*");
  const body = await res.json();
  expect(Object.keys(body.results[0]).sort()).toEqual(["birthYear", "cemetery", "cemeteryId", "deathYear", "lat", "lon", "name", "plot"]);
});
