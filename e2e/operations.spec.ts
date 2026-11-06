import { expect, test } from "@playwright/test";
import { login, nav, openedPdf } from "./helpers";

// W8–W11: chapel booking with conflict check, recurring work order, case file, inspection.
test("üzemeltetés: foglalás ütközéssel, munkalap, ügyirat, bejárás", async ({ page }) => {
  await login(page);
  await nav(page, "Üzemeltetés");

  // facility + booking; an overlapping booking of the same chapel is refused
  await page.getByRole("button", { name: "Helyiségek" }).click();
  await page.locator("input[name=facilityName]").fill("Ravatalozó");
  await page.getByRole("dialog").getByRole("button", { name: "Hozzáadás" }).click();
  await expect(page.getByRole("dialog").getByText("Ravatalozó · Ravatalozó")).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Bezárás" }).last().click();

  await page.getByRole("button", { name: "Új időpont" }).click();
  await page.locator("input[name=title]").fill("Teszt Elek temetése");
  await expect(page.getByRole("dialog").getByLabel("Helyiség")).toHaveValue(/.+/);
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(page.getByTestId("calendar")).toContainText("Teszt Elek temetése");

  await page.getByRole("button", { name: "Új időpont" }).click();
  await page.locator("input[name=title]").fill("Ütköző szertartás");
  await expect(page.getByRole("dialog").getByLabel("Helyiség")).toHaveValue(/.+/);
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(page.getByRole("dialog").getByText(/Ez az időpont már foglalt: Teszt Elek temetése/)).toBeVisible();
  await page.locator("input[name=start]").fill("11:00");
  await page.locator("input[name=end]").fill("12:00");
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(page.getByTestId("calendar")).toContainText("Ütköző szertartás");

  // recurring work order: closing it creates the next one
  await page.getByRole("tab", { name: "Munkalapok" }).click();
  await page.getByRole("button", { name: "Új munkalap" }).click();
  await page.locator("input[name=title]").fill("Fűnyírás a főúton");
  await page.locator("select[name=recurrence]").selectOption("monthly");
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  const card = page.getByTestId("work-orders").getByRole("listitem").filter({ hasText: "Fűnyírás a főúton" });
  await expect(card).toHaveCount(1);
  await card.getByRole("button", { name: "Elvégezve" }).click();
  await page.locator("textarea[name=note]").fill("kész");
  await page.getByRole("dialog").getByRole("button", { name: "Elvégezve" }).click();
  await expect(card).toHaveCount(1);
  await expect(card).toContainText("Havonta");
  await page.getByRole("button", { name: "Lezárt" }).click();
  await expect(page.getByTestId("work-orders")).toContainText("kész");

  // case file: gap-free number, decision required for approval
  await page.getByRole("tab", { name: "Ügyiratok" }).click();
  await page.getByRole("button", { name: "Új ügy iktatása" }).click();
  await page.locator("input[name=subject]").fill("Síremlék állítása Teszt Elek sírjára");
  await page.locator("input[name=applicant]").fill("Teszt Ügyfél");
  await page.getByRole("button", { name: "Iktatás", exact: true }).click();
  const year = new Date().getFullYear();
  await expect(page.getByRole("dialog").getByRole("heading", { name: new RegExp(`^\\d+/${year} · Síremlék állítása`) })).toBeVisible();
  await page.locator("select[name=caseStatus]").selectOption("approved");
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(page.getByRole("dialog").getByText(/írja be a döntést/)).toBeVisible();
  await page.locator("textarea[name=decision]").fill("A terv szerint engedélyezve.");
  await page.getByRole("dialog").getByRole("button", { name: "Mentés" }).click();
  await expect(page.getByRole("dialog").getByText("Módosítva")).toBeVisible();
  // the decision is issued on the letterhead and kept with the case
  expect(await openedPdf(page, () => page.getByRole("dialog").getByRole("button", { name: "Irat kiadása (PDF)" }).click())).toBe("%PDF-");
  await expect(page.getByRole("dialog").getByRole("button", { name: /^hatarozat.*\.pdf$/ })).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Bezárás" }).last().click();
  await expect(page.getByTestId("cases").getByRole("row").filter({ hasText: "Teszt Ügyfél" })).toContainText("Jóváhagyva");

  // inspection: a neglected plot opens a neglect case
  await page.getByRole("tab", { name: "Bejárás" }).click();
  await page.getByRole("list", { name: "Válassza ki a helyet" }).getByRole("radio").first().check();
  await page.getByRole("button", { name: "Állapot mentése" }).click();
  await expect(page.getByText("Állapot rögzítve, az ügy iktatva.")).toBeVisible();
  await expect(page.getByTestId("conditions")).toContainText("Gondozatlan");
});
