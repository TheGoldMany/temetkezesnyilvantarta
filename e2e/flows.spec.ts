import { expect, test } from "@playwright/test";
import path from "node:path";
import { login, nav, openedPdf } from "./helpers";

test.describe.configure({ mode: "serial" });

test("UC-01: új temetés új helyre, folyószámmal és megváltással", async ({ page }) => {
  await login(page);
  await page.getByRole("button", { name: "Új temetés rögzítése" }).first().click();
  // 1. elhunyt
  await page.getByLabel("Családi név").fill("Teszt");
  await page.getByLabel("Utónév").fill("Tivadar");
  await page.getByLabel("Születési idő").fill("1941.07.12");
  await page.getByLabel("Anyja neve").fill("Minta Margit");
  await page.getByLabel("Legutolsó lakóhely").fill("9999 Mintavölgy, Béke utca 4.");
  await page.getByLabel("Elhalálozás ideje").fill("20261101");
  await page.getByRole("button", { name: "Tovább" }).click();
  // 2. hely – csak szabad / lefoglalt hely választható
  await page.getByRole("textbox", { name: "Válassza ki a helyet" }).fill("C/4");
  await page.locator("label").filter({ hasText: "Szabad" }).first().click();
  await expect(page.getByTestId("verdict")).toContainText("engedélyezhető");
  await page.getByRole("button", { name: "Tovább" }).click();
  // 3. eltemettető (= jogosult)
  await page.getByLabel("Családi név").fill("Teszt");
  await page.getByLabel("Utónév").fill("Tímea");
  await page.getByLabel("Lakcím").fill("9999 Mintavölgy, Béke utca 4.");
  await page.getByRole("button", { name: "Tovább" }).click();
  // 4. megváltás – javasolt 25 év
  await expect(page.getByText(/Javasolt: Megváltás/)).toBeVisible();
  await page.getByRole("button", { name: "Tovább" }).click();
  // 5. ellenőrzés és mentés
  await page.getByRole("button", { name: "Temetés rögzítése" }).click();
  await expect(page.getByTestId("burial-done")).toContainText(/Rögzítve a \d+\. folyószámon/);
  // nyomtatványcsomag: előre kijelölve, egy PDF-ben
  const pack = page.getByTestId("burial-pack");
  await expect(pack.getByLabel(/Gyászlap/)).toBeChecked();
  await expect(pack.getByLabel(/Sírásási munkalap/)).toBeChecked();
  expect(await openedPdf(page, () => pack.getByRole("button", { name: /Kijelöltek nyomtatása/ }).click())).toBe("%PDF-");
  await page.getByRole("button", { name: "Hely adatlapja" }).click();
  await expect(page.getByText("Teszt Tivadar").first()).toBeVisible();
  await expect(page.getByText("Foglalt").first()).toBeVisible();
  await expect(page.getByText("Teszt Tímea").first()).toBeVisible();
});

test("urna hazavitele sírhely nélkül: folyószám, átvevő", async ({ page }) => {
  await login(page);
  await page.getByRole("button", { name: "Új temetés rögzítése" }).first().click();
  await page.getByLabel("Családi név").fill("Hazavitt");
  await page.getByLabel("Utónév").fill("Hermina");
  await page.getByLabel("Születési idő").fill("1938.03.02");
  await page.getByLabel("Anyja neve").fill("Minta Margit");
  await page.getByLabel("Legutolsó lakóhely").fill("9999 Mintavölgy, Béke utca 6.");
  await page.getByLabel("Elhalálozás ideje").fill("20261020");
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.locator("select[name=eventKind]").selectOption("off_home");
  await expect(page.getByRole("button", { name: "Tovább" })).toBeDisabled();
  await page.locator("input[name=receiverName]").fill("Hazavitt Gergely");
  await page.locator("input[name=receiverAddress]").fill("9999 Mintavölgy, Béke utca 6.");
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByRole("button", { name: "Tovább" }).click();
  await expect(page.getByText(/megváltás sincs/)).toBeVisible();
  await page.getByRole("button", { name: "Tovább" }).click();
  await expect(page.getByText("Hazavitt Gergely, 9999 Mintavölgy")).toBeVisible();
  await page.getByRole("button", { name: "Temetés rögzítése" }).click();
  await expect(page.getByTestId("burial-done")).toContainText(/Rögzítve a \d+\. folyószámon/);
});

test("UC-02: rátemetés – nyugvási időn belül koporsó tiltva, urna engedélyezve", async ({ page }) => {
  await login(page);
  // keresés ékezet nélkül a gyorskeresőben
  await page.getByRole("combobox", { name: "Keresés" }).fill("teszt tivadar");
  await page.locator("#quicksearch-results").getByRole("option").first().click();
  await page.getByRole("button", { name: "Hely adatlapja" }).click();
  await page.getByRole("button", { name: "Rátemetés" }).click();
  await page.getByLabel("Családi név").fill("Teszt");
  await page.getByLabel("Utónév").fill("Teréz");
  await page.getByLabel("Születési idő").fill("1944.02.02");
  await page.getByLabel("Anyja neve").fill("Minta Mária");
  await page.getByLabel("Legutolsó lakóhely").fill("9999 Mintavölgy");
  await page.getByLabel("Elhalálozás ideje").fill("20261102");
  await page.getByRole("button", { name: "Tovább" }).click();
  await expect(page.getByTestId("verdict")).toContainText("nem engedélyezhető");
  await expect(page.getByTestId("verdict")).toContainText("nyugvási idő");
  // urna: engedélyezett
  await page.getByLabel("Esemény").selectOption("urn");
  await expect(page.getByTestId("verdict")).toContainText("A temetés engedélyezhető.");
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByLabel("Családi név").fill("Teszt");
  await page.getByLabel("Utónév").fill("Tamás");
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByRole("button", { name: "Temetés rögzítése" }).click();
  await expect(page.getByTestId("burial-done")).toBeVisible();
});

test("UC-03: lejáratkezelés – értesítés rögzítése és levelek nyomtatása", async ({ page }) => {
  await login(page);
  await nav(page, "Lejáratok");
  await page.getByLabel("Csak az értesítetlenek").check();
  const rows = page.locator("tbody tr");
  await expect(rows.first()).toBeVisible();
  const before = await rows.count();
  await rows.nth(0).locator("input[type=checkbox]").check();
  await rows.nth(1).locator("input[type=checkbox]").check();
  await expect(page.getByTestId("bulk-bar")).toContainText("2 kijelölve");
  // értesítő levelek PDF
  expect(await openedPdf(page, () => page.getByRole("button", { name: "Értesítő levelek" }).click())).toBe("%PDF-");
  await page.getByRole("button", { name: "Értesítés rögzítése" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Értesítés rögzítése" }).click();
  await expect(page.getByText("2 értesítés rögzítve.")).toBeVisible();
  await expect(rows).toHaveCount(before - 2);
});

test("UC-05: átállás – Excel/CSV import próbafuttatással, gyors bevitel, ellenőrzés", async ({ page }) => {
  await login(page);
  await nav(page, "Import és export");
  await page.getByTestId("import-file").setInputFiles(path.resolve("e2e/fixtures/regi-konyv.csv"));
  await expect(page.getByText("3 adatsor")).toBeVisible();
  // automatikus oszlop-hozzárendelés
  await expect(page.getByLabel("Folyószám", { exact: true })).toHaveValue("0");
  await page.getByRole("button", { name: "Próbafuttatás" }).click();
  await expect(page.getByTestId("import-report")).toContainText("2 sor rendben, 1 hibás");
  await expect(page.getByTestId("import-report")).toContainText("3. sor");
  // minden vagy semmi: hibás sorok kihagyásával betöltés
  await page.getByLabel("Hibás sorok kihagyása").check();
  await page.getByRole("button", { name: "Próbafuttatás" }).click();
  await page.getByRole("button", { name: "Betöltés", exact: true }).click();
  await expect(page.getByTestId("import-report")).toContainText("Betöltve: 2 sor");
  // a régi folyószám megmaradt
  await page.getByRole("combobox", { name: "Keresés" }).fill("regi rozalia");
  await page.locator("#quicksearch-results").getByRole("option").first().click();
  await expect(page.getByText("9001.")).toBeVisible();
  // gyors bevitel papírkönyvből, billentyűzettel
  await nav(page, "Import és export");
  await page.getByRole("button", { name: "Gyors bevitel papírkönyvből" }).click();
  const form = page.getByTestId("quick-entry");
  await form.getByLabel("Régi (papír) folyószám").fill("9010");
  await page.keyboard.press("Enter");
  await page.keyboard.type("1961.05.06");
  await form.getByLabel(/Családi név és utónév/).fill("Gyors Gizella");
  await form.getByLabel("Születési idő").fill("1899.01.01");
  await form.getByLabel("Anyja neve").fill("Minta Mária");
  await form.getByLabel("Elhalálozás ideje").fill("1961.05.03");
  await form.getByLabel("Legutolsó lakóhely").fill("9999 Mintavölgy");
  await form.getByLabel("Hely azonosítója").fill("R/2/1");
  await page.keyboard.press("Control+Enter");
  await expect(page.getByTestId("quick-recent")).toContainText("9010. Gyors Gizella");
  // átállási ellenőrző riport
  await nav(page, "Import és export");
  await page.getByRole("button", { name: "Ellenőrzés" }).click();
  await expect(page.getByTestId("migration-check")).toContainText("Elhunytak hely nélkül: 0");
});

test("Hiteles nyilvántartó könyv nyomtatása", async ({ page }) => {
  await login(page);
  await nav(page, "Nyomtatványok");
  await page.getByLabel("ettől").fill("1950.01.01");
  expect(await openedPdf(page, () => page.getByRole("button", { name: "PDF készítése" }).first().click())).toBe("%PDF-");
  await expect(page.getByText(/Kötet azonosító/)).toBeVisible();
});

test("díjtáblázat a hirdetőtáblára", async ({ page }) => {
  await login(page);
  await nav(page, "Nyomtatványok");
  const card = page.locator("section, div").filter({ has: page.getByRole("heading", { name: "Díjtáblázat a hirdetőtáblára" }) }).last();
  expect(await openedPdf(page, () => card.getByRole("button", { name: "PDF készítése" }).click())).toBe("%PDF-");
});
