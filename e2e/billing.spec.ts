import { expect, test, type Page } from "@playwright/test";
import { lastMail, login } from "./helpers";

async function registerAndStart(page: Page, email: string, org: string) {
  await page.goto("/");
  await page.getByRole("button", { name: "Kipróbálom 30 napig ingyen" }).click();
  await page.locator("input[name=organization]").fill(org);
  await page.locator("input[name=name]").fill("Pénzügy Piroska");
  await page.locator("input[name=email]").fill(email);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.locator("input[name=password2]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Fiók létrehozása" }).click();
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByRole("button", { name: /Üres nyilvántartással/ }).click();
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
}

const rpc = (page: Page, cmd: string, args: object = {}) =>
  page.evaluate(
    async ([c, a]) => (await fetch("/rpc", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ cmd: c, args: a }) })).json(),
    [cmd, args] as const,
  );

test("előfizetés átutalással: díjbekérő, befizetés, számla", async ({ page, browser }) => {
  const email = `penzugy-${Date.now()}@pelda.hu`;
  await registerAndStart(page, email, "Kisrét Község Önkormányzata");

  // a new cemetery shows its effect on the price first
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Beállítások", exact: true }).click();
  await page.getByRole("tab", { name: "Temetők és parcellák" }).click();
  await page.getByRole("button", { name: "Új temető" }).click();
  await expect(page.getByText(/A próbaidő alatt a temetők száma nem számít/)).toBeVisible();
  await page.getByRole("button", { name: "Mégse" }).click();

  await page.getByRole("tab", { name: "Előfizetés" }).click();
  await expect(page.getByTestId("billing-status")).toContainText("Próbaidő");
  await page.getByRole("radio", { name: /Éves díj/ }).click();
  await expect(page.getByRole("radio", { name: /Éves díj/ })).toContainText("450");
  await page.locator("input[name=postcode]").fill("8999");
  await page.locator("input[name=city]").fill("Kisrét");
  await page.locator("input[name=address]").fill("Fő utca 1.");
  await page.locator("input[name=taxNumber]").fill("15727000-2-20");
  await page.getByRole("button", { name: "Számlázási adatok mentése" }).click();
  await page.getByRole("button", { name: "Előfizetek, kérem a díjbekérőt" }).click();
  await expect(page.getByText(/Elküldtük a díjbekérőt/)).toBeVisible();
  await expect(page.getByTestId("invoices")).toContainText("Díjbekérő");
  await expect(page.getByTestId("invoices")).toContainText("571");
  const proforma = await lastMail(page, email, "proforma");
  expect(proforma.body).toContain("Fizetési határidő");

  // the TEMISZ operator matches the transfer
  const admin = await (await browser.newContext()).newPage();
  await login(admin);
  await admin.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Ügyfelek", exact: true }).click();
  const row = admin.getByTestId("open-proformas").getByRole("row").filter({ hasText: "Kisrét Község" });
  await row.getByRole("button", { name: "Befizetve" }).click();
  await expect(admin.getByTestId("open-proformas").getByRole("row").filter({ hasText: "Kisrét Község" })).toHaveCount(0);

  await page.reload();
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Beállítások", exact: true }).click();
  await page.getByRole("tab", { name: "Előfizetés" }).click();
  await expect(page.getByTestId("billing-status")).toContainText("Kifizetve");
  await expect(page.getByTestId("invoices")).toContainText("Számla");
  expect((await lastMail(page, email, "invoice")).subject).toContain("TEMISZ számla");
});

test("lejárt próbaidő: csak olvasható, olvasni lehet", async ({ page }) => {
  const email = `lejart-${Date.now()}@pelda.hu`;
  await registerAndStart(page, email, "Lejártfalva");
  await page.request.post(`/dev/expire-trial?email=${encodeURIComponent(email)}`);
  await page.reload();
  await expect(page.getByTestId("readonly-banner")).toBeVisible();
  const write = await rpc(page, "api", { method: "settings.set", args: { values: { "operator.address": "x" } } });
  expect(write.err.code).toBe("read_only");
  const read = await rpc(page, "api", { method: "cemeteries.list", args: {} });
  expect(read.ok).toEqual([]);
  const trialEnded = await page.request.get("/api/cron/daily");
  expect(trialEnded.ok()).toBeTruthy();
});

test("fiók megszüntetése: belépés tiltva, adatletöltés előtte", async ({ page }) => {
  const email = `megszunik-${Date.now()}@pelda.hu`;
  await registerAndStart(page, email, "Megszűnőfalva");
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Beállítások", exact: true }).click();
  await page.getByRole("tab", { name: "Mentés és visszaállítás" }).click();
  await expect(page.getByRole("button", { name: "Minden adat letöltése" })).toBeVisible();
  await expect(page.getByText("Átállás a régi asztali TEMISZ-ről")).toBeVisible();
  await page.getByRole("tab", { name: "Előfizetés" }).click();
  await page.getByRole("button", { name: "Fiók megszüntetése…" }).click();
  await page.getByRole("checkbox").last().check();
  await page.locator("input[name=closePassword]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Fiók végleges megszüntetése" }).click();
  await expect(page.locator("input[name=email]")).toBeVisible();
  expect((await lastMail(page, email, "closed")).body).toContain("30 nap");
  await page.locator("input[name=email]").fill(email);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Belépés" }).click();
  await expect(page.getByText("Ez a fiók megszűnt.")).toBeVisible();
});
