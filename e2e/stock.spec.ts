import { expect, test } from "@playwright/test";
import { login, nav } from "./helpers";

// Goods at the office: a new item, goods in, a sale for invoicing (the cash
// receipt path is covered by the unit test: other tests close today's cash
// day of the sample cemetery), and a stocktake with a difference.
test("készlet: beérkezés, eladás, leltár", async ({ page }) => {
  await login(page);
  await nav(page, "Pénzügy");
  await page.getByRole("tab", { name: "Készlet" }).click();
  const name = `Mécses ${Date.now() % 100000}`;

  await page.getByRole("button", { name: "Új termék" }).click();
  await page.locator("input[name=itemName]").fill(name);
  await page.locator("input[name=unitNetHuf]").fill("400");
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  const row = page.getByTestId("stock-table").getByRole("row").filter({ hasText: name });
  await expect(row).toContainText("508 Ft");
  await expect(row).toContainText("0 db");

  await row.getByRole("button", { name: "Beérkezés" }).click();
  await page.locator("input[name=qty]").fill("20");
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(row).toContainText("20 db");

  // a sale for invoicing: the items appear among the charges
  await page.getByRole("button", { name: "Eladás", exact: true }).click();
  const sale = page.getByRole("dialog");
  await sale.getByLabel(`${name}: Mennyiség`).fill("3");
  await sale.locator("input[name=payerName]").fill("Kiss Margit");
  await expect(sale).toContainText(/Összesen: 1\D?524\sFt/);
  await sale.getByRole("combobox").selectOption("invoice");
  await sale.getByRole("button", { name: "Tételként rögzítem" }).click();
  await expect(page.getByText(/Eladás rögzítve: 1\D?524\sFt/)).toBeVisible();
  await expect(row).toContainText("17 db");
  // more than in stock is refused
  await page.getByRole("button", { name: "Eladás", exact: true }).click();
  await page.getByRole("dialog").getByLabel(`${name}: Mennyiség`).fill("99");
  await page.getByRole("dialog").locator("input[name=payerName]").fill("Kiss Margit");
  await page.getByRole("dialog").getByRole("combobox").selectOption("invoice");
  await page.getByRole("dialog").getByRole("button", { name: "Tételként rögzítem" }).click();
  await expect(page.getByRole("dialog")).toContainText("Nincs elég készleten");
  await page.getByRole("dialog").getByRole("button", { name: "Mégse" }).click();

  // stocktake: 15 counted
  await page.getByRole("button", { name: "Leltár" }).click();
  await page.getByRole("dialog").getByLabel(`${name}: Megszámolva`).fill("15");
  await expect(page.getByRole("dialog")).toContainText("-2");
  await page.getByRole("dialog").getByRole("button", { name: /Leltár mentése/ }).click();
  await expect(page.getByText(/Leltár mentve: 1 tétel, 1 eltérés/)).toBeVisible();
  await expect(row).toContainText("15 db");

  // the movements of the item
  await row.getByRole("button", { name: "Mozgások" }).click();
  const moves = page.getByRole("dialog");
  await expect(moves).toContainText("Beérkezés");
  await expect(moves).toContainText("Eladás");
  await expect(moves).toContainText("Leltári eltérés");
});
