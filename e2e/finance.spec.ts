import { expect, test } from "@playwright/test";
import { login, nav } from "./helpers";

// W7: item → invoice → cash payment → cash book → day closing.
test("pénzügy: tétel, számla, készpénzes befizetés, pénztárzárás", async ({ page }) => {
  await login(page);
  await nav(page, "Pénzügy");
  await page.getByRole("button", { name: "Új tétel" }).click();
  await page.locator("input[name=payerName]").fill("Teszt Befizető");
  await page.locator("input[name=description]").fill("Ravatalozó használata");
  await page.locator("input[name=price]").fill("20000");
  await page.getByRole("button", { name: "Mentés" }).click();
  const row = page.getByTestId("charges").getByRole("row").filter({ hasText: "Teszt Befizető" });
  await expect(row).toContainText("25 400");
  await row.getByRole("checkbox").check();
  await page.getByRole("button", { name: /Számla \(1 tétel\)/ }).click();
  await page.locator("input[name=buyerPostcode]").fill("9999");
  await page.locator("input[name=buyerCity]").fill("Mintavölgy");
  await page.locator("input[name=buyerAddress]").fill("Fő utca 1.");
  await page.getByRole("button", { name: "Számla kiállítása" }).click();
  await expect(page.getByTestId("charges").getByRole("row").filter({ hasText: "Teszt Befizető" })).toHaveCount(0);

  await page.getByRole("tab", { name: "Számlák" }).click();
  const inv = page.getByTestId("invoices-table").getByRole("row").filter({ hasText: "Teszt Befizető" });
  await expect(inv).toContainText("TESZT-S-");
  await expect(inv).toContainText("Fizetendő");
  await inv.getByRole("button", { name: "Befizetés" }).click();
  await page.getByRole("dialog").getByLabel("Fizetés módja").selectOption("cash");
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(inv).toContainText("Kifizetve");

  await page.getByRole("tab", { name: "Házipénztár" }).click();
  const year = new Date().getFullYear();
  await expect(page.getByTestId("cash-entries")).toContainText(`BE-${year}/`);
  await expect(page.getByTestId("cash-summary")).toContainText("25 400");
  await page.locator("input[name=counted]").fill("25000");
  await page.getByRole("button", { name: "Nap lezárása" }).click();
  await expect(page.getByText(/Írja be az eltérés okát/)).toBeVisible();
  await page.locator("input[name=counted]").fill("25400");
  await page.getByRole("button", { name: "Nap lezárása" }).click();
  await expect(page.getByText(/A nap lezárva/)).toBeVisible();

  await page.getByRole("tab", { name: "Kimutatás" }).click();
  await expect(page.getByTestId("revenue-totals")).toContainText("25 400");
});
