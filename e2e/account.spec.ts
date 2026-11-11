import { expect, test, type Page } from "@playwright/test";
import { lastMail, linkIn, totp } from "./helpers";

async function register(page: Page, email: string, org = "Próbafalva Önkormányzata") {
  await page.goto("/");
  await page.getByRole("button", { name: "Kipróbálom 30 napig ingyen" }).click();
  await page.locator("input[name=organization]").fill(org);
  await page.locator("input[name=name]").fill("Teszt Elek");
  await page.locator("input[name=email]").fill(email);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.locator("input[name=password2]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Fiók létrehozása" }).click();
  await expect(page.getByLabel("Üzemeltető neve")).toHaveValue(org, { timeout: 15_000 });
}

async function finishOnboarding(page: Page) {
  await page.getByRole("button", { name: "Tovább" }).click();
  await page.getByRole("button", { name: /Kezdés üres|Üres nyilvántartással/ }).click();
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
  // the guided tour opens after the first-run wizard
  await page.getByTestId("tour").getByRole("button", { name: "Kihagyom" }).click();
}

const rpc = (page: Page, cmd: string, args: object = {}) =>
  page.evaluate(
    async ([c, a]) => (await fetch("/rpc", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ cmd: c, args: a }) })).json(),
    [cmd, args] as const,
  );

test("regisztráció: saját, üres nyilvántartás, e-mail-megerősítés", async ({ page }) => {
  const email = `teszt-${Date.now()}@pelda.hu`;
  await register(page, email);
  await expect(page.getByLabel("Aláíró neve")).toHaveValue("Teszt Elek");

  // another customer's people are not searchable from this account
  expect((await rpc(page, "api", { method: "search.quick", args: { query: "kovacs", limit: 30 } })).ok).toEqual([]);

  await finishOnboarding(page);
  await expect(page.getByRole("status").filter({ hasText: /Erősítse meg az e-mail-címét/ })).toBeVisible();
  await expect(page.getByTestId("trial-banner")).toContainText("még 30 nap");
  const mail = await lastMail(page, email, "verify");
  await page.goto(linkIn(mail.body));
  await expect(page.getByText("Az e-mail-cím megerősítve.")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: /Erősítse meg az e-mail-címét/ })).toHaveCount(0);

  await rpc(page, "logout");
  expect((await rpc(page, "api", { method: "search.quick", args: { query: "a", limit: 1 } })).err.code).toBe("session_expired");
});

test("meghívó, elfelejtett jelszó, kétlépcsős belépés", async ({ page, browser }) => {
  const admin = `admin-${Date.now()}@pelda.hu`;
  const colleague = `kolléga-${Date.now()}@pelda.hu`.replace("é", "e");
  await register(page, admin, "Hegyalja Egyházközség");
  await finishOnboarding(page);

  // invite a colleague
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Beállítások", exact: true }).click();
  await page.getByRole("tab", { name: "Felhasználók" }).click();
  await page.getByRole("button", { name: "Kolléga meghívása" }).click();
  await page.getByLabel(/Teljes név/).fill("Kovács Kolléga");
  await page.locator("input[name=email]").fill(colleague);
  await page.getByRole("button", { name: "Meghívó küldése" }).click();
  await expect(page.getByText("Kovács Kolléga")).toBeVisible();

  // the colleague accepts in another browser
  const other = await (await browser.newContext()).newPage();
  const invite = await lastMail(page, colleague, "invite");
  await other.goto(linkIn(invite.body));
  await other.locator("input[name=password]").fill("kollega-jelszo-1");
  await other.locator("input[name=password2]").fill("kollega-jelszo-1");
  await other.getByRole("button", { name: "Fiók aktiválása" }).click();
  await expect(other.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
  // the colleague's first sign-in starts the guided tour; once closed it stays closed
  await expect(other.getByTestId("tour")).toContainText("Üdvözöljük");
  await other.getByTestId("tour").getByRole("button", { name: "Kihagyom" }).click();
  await expect(other.getByTestId("tour")).toHaveCount(0);

  // forgotten password
  await rpc(other, "logout");
  await other.goto("/");
  await other.getByRole("button", { name: "Elfelejtette a jelszavát?" }).click();
  await other.locator("input[name=email]").fill(colleague);
  await other.getByRole("button", { name: "Hivatkozás küldése" }).click();
  await expect(other.getByText(/elküldtük rá a hivatkozást/)).toBeVisible();
  const reset = await lastMail(page, colleague, "reset");
  await other.goto(linkIn(reset.body));
  await other.locator("input[name=password]").fill("uj-kollega-jelszo-2");
  await other.locator("input[name=password2]").fill("uj-kollega-jelszo-2");
  await other.getByRole("button", { name: "Jelszó mentése és belépés" }).click();
  await expect(other.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
  await expect(other.getByTestId("tour")).toHaveCount(0);

  // with a colleague on board, the admin must turn on two-factor sign-in
  await rpc(page, "logout");
  await page.goto("/");
  await page.locator("input[name=email]").fill(admin);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Belépés" }).click();
  await expect(page.getByRole("heading", { name: "Kapcsolja be a kétlépcsős azonosítást" })).toBeVisible();
  expect((await rpc(page, "api", { method: "cemeteries.list", args: {} })).err.code).toBe("totp_required");
  const secret = (await page.getByTestId("totp-secret").innerText()).replace(/\s/g, "");
  await page.locator("input[name=code]").fill(totp(secret));
  await page.getByRole("button", { name: "Bekapcsolás" }).click();
  await expect(page.getByTestId("recovery-codes")).toBeVisible();
  const recovery = (await page.getByTestId("recovery-codes").locator("li").first().innerText()).trim();
  await page.getByRole("button", { name: "Elmentettem a kódokat" }).click();
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();

  // next sign-in asks for the code; a wrong one is refused, a recovery code works once
  await rpc(page, "logout");
  await page.goto("/");
  await page.locator("input[name=email]").fill(admin);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Belépés" }).click();
  await page.locator("input[name=code]").fill("000000");
  await page.getByRole("button", { name: "Belépés" }).click();
  await expect(page.getByText(/A kód nem helyes/)).toBeVisible();
  await page.locator("input[name=code]").fill(recovery);
  await page.getByRole("button", { name: "Belépés" }).click();
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();

  // the sign-in log shows the colleague's password reset and the wrong code
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: "Beállítások", exact: true }).click();
  await page.getByRole("tab", { name: "Fiókom" }).click();
  const log = page.getByTestId("security-log");
  await expect(log).toContainText("Hibás azonosító kód");
  await expect(log).toContainText("Új jelszó e-mailes hivatkozással");
  await expect(log).toContainText("Kétlépcsős azonosítás bekapcsolva");
});

test("idegen oldalról érkező kérés elutasítva", async ({ request }) => {
  const r = await request.post("/rpc", { headers: { "Content-Type": "application/json", Origin: "https://tamado.example" }, data: { cmd: "app_status", args: {} } });
  expect(r.status()).toBe(403);
});
