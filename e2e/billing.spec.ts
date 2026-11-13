import { expect, test, type Page } from "@playwright/test";
import { lastMail, login } from "./helpers";

async function registerAndStart(page: Page, email: string, org: string) {
  await page.goto("/");
  await page.getByRole("button", { name: "Ingyenes regisztráció" }).click();
  await page.locator("input[name=organization]").fill(org);
  await page.locator("input[name=name]").fill("Pénzügy Piroska");
  await page.locator("input[name=email]").fill(email);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.locator("input[name=password2]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Fiók létrehozása" }).click();
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByRole("button", { name: /Üres nyilvántartással/ }).click();
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
  await page.getByTestId("tour").getByRole("button", { name: "Kihagyom" }).click();
}

const rpc = (page: Page, cmd: string, args: object = {}) =>
  page.evaluate(
    async ([c, a]) => (await fetch("/rpc", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ cmd: c, args: a }) })).json(),
    [cmd, args] as const,
  );

test("temető előfizetése átutalással: díjbekérő, befizetés, számla", async ({ page, browser }) => {
  const email = `penzugy-${Date.now()}@pelda.hu`;
  // unique per run: the database is kept between local runs
  const org = `Kisrét Község ${Date.now() % 1000000}`;
  await registerAndStart(page, email, `${org} Önkormányzata`);

  // the empty organisation is led through its first steps
  const first = page.getByTestId("first-steps");
  await expect(first).toContainText("Első lépések");
  await expect(page.getByRole("button", { name: "Új temetés rögzítése" })).toHaveCount(0);
  await first.getByRole("button", { name: "Temető létrehozása" }).click();
  // a new cemetery needs its own subscription: until then it can only be read
  await page.getByRole("button", { name: "Új temető" }).click();
  await expect(page.getByText(/Minden temetőnek saját előfizetése van/)).toBeVisible();
  await page.getByLabel(/Temető neve/).fill("Kisréti köztemető");
  await page.getByRole("button", { name: "Mentés" }).click();
  // the next step is the subscription
  const subs = page.getByTestId("cemetery-subs");
  await expect(subs).toContainText("Kisréti köztemető");
  await expect(subs).toContainText("Nincs előfizetés");
  await expect(page.getByTestId("locked-banner")).toBeVisible();
  const cem = (await rpc(page, "api", { method: "cemeteries.list", args: {} })).ok[0].id;
  const locked = await rpc(page, "api", { method: "structure.bulkCreate", args: { input: { cemeteryId: cem, sectionCode: "A", rowFrom: 1, rowTo: 1, plotsPerRow: 2, firstPlotNumber: 1, plotTypeId: "x" } } });
  expect(locked.err.code).toBe("cemetery_locked");

  await page.locator("input[name=postcode]").fill("8999");
  await page.locator("input[name=city]").fill("Kisrét");
  await page.locator("input[name=address]").fill("Fő utca 1.");
  await page.locator("input[name=taxNumber]").fill("15727000-2-20");
  await page.getByRole("button", { name: "Számlázási adatok mentése" }).click();
  await subs.getByRole("button", { name: "Előfizetés" }).click();
  const form = page.getByTestId("subscribe-form");
  await form.getByRole("radio", { name: /Éves díj/ }).click();
  await expect(form.getByRole("radio", { name: /Éves díj/ })).toContainText("450");
  // the ÁSZF must be accepted first
  await expect(form.getByRole("button", { name: "Előfizetek, kérem a díjbekérőt" })).toBeDisabled();
  await form.getByTestId("terms").check();
  await form.getByRole("button", { name: "Előfizetek, kérem a díjbekérőt" }).click();
  await expect(form).toHaveCount(0);
  await expect(subs).toContainText("Díjbekérő kiküldve");
  await expect(page.getByTestId("locked-banner")).toHaveCount(0);
  await expect(page.getByTestId("invoices")).toContainText("Díjbekérő");
  await expect(page.getByTestId("invoices")).toContainText("571");
  const proforma = await lastMail(page, email, "proforma");
  expect(proforma.body).toContain("Fizetési határidő");
  // usable at once, but nothing can be exported before the payment arrives
  await expect(subs).toContainText("a befizetésre várunk");
  expect((await rpc(page, "api", { method: "export.file", args: { kind: "full", format: "xlsx" } })).err.code).toBe("trial_no_export");

  // the TEMISZ operator matches the transfer
  const admin = await (await browser.newContext()).newPage();
  await login(admin);
  await admin.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Ügyfelek", exact: true }).click();
  // … by loading the bank statement: the reference carries the customer code
  const row = admin.getByTestId("open-proformas").getByRole("row").filter({ hasText: org });
  const code = (await row.innerText()).match(/Közlemény: ([0-9a-f]{8})/)![1];
  const csv = `Könyvelés dátuma;Összeg;Partner neve;Közlemény\n2026.11.06;571 500;KISRÉT KÖZSÉG ÖNK.;TEMISZ ${code}\n`;
  await admin.locator("input[name=statement]").setInputFiles({ name: "kivonat.csv", mimeType: "text/csv", buffer: Buffer.from(csv, "utf-8") });
  await expect(admin.getByText(/1 díjbekérő automatikusan kiegyenlítve/)).toBeVisible();
  await expect(admin.getByTestId("open-proformas").getByRole("row").filter({ hasText: org })).toHaveCount(0);

  await page.reload();
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Beállítások", exact: true }).click();
  await page.getByRole("tab", { name: "Előfizetés" }).click();
  await expect(page.getByTestId("invoices")).toContainText("Számla");
  await expect(page.getByTestId("cemetery-subs")).toContainText(/Előfizetve .* átutalással/);
  expect((await lastMail(page, email, "invoice")).subject).toContain("TEMISZ számla");
  // paid: the data can be exported
  expect((await rpc(page, "api", { method: "export.file", args: { kind: "full", format: "xlsx" } })).ok.fileName).toContain(".xlsx");
});

test("mintatemető: ingyenes, de export nélkül, vízjeles nyomtatással", async ({ page }) => {
  const email = `minta-${Date.now()}@pelda.hu`;
  await page.goto("/");
  await page.getByRole("button", { name: "Ingyenes regisztráció" }).click();
  await page.locator("input[name=organization]").fill("Mintafalva");
  await page.locator("input[name=name]").fill("Minta Márta");
  await page.locator("input[name=email]").fill(email);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.locator("input[name=password2]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Fiók létrehozása" }).click();
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByRole("button", { name: "Mintatemető betöltése" }).click();
  await page.getByRole("button", { name: "Kezdés" }).click();
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
  // writable
  expect((await rpc(page, "api", { method: "settings.set", args: { values: { "operator.address": "Minta utca 1." } } })).err).toBeUndefined();
  // no data export, no authenticated books
  expect((await rpc(page, "api", { method: "export.file", args: { kind: "full", format: "xlsx" } })).err.code).toBe("trial_no_export");
  const cem = (await rpc(page, "api", { method: "cemeteries.list", args: {} })).ok[0].id;
  expect((await rpc(page, "api", { method: "print.document", args: { doc: "register", args: { cemeteryId: cem } } })).err.code).toBe("trial_no_export");
  // other prints work, marked
  const map = await rpc(page, "api", { method: "print.document", args: { doc: "map", args: { cemeteryId: cem, paper: "A4" } } });
  expect(map.ok.pages).toBeGreaterThan(0);
  const daily = await page.request.get("/api/cron/daily");
  expect(daily.ok()).toBeTruthy();
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
