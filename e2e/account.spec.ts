import { expect, test } from "@playwright/test";

// W1: a new customer registers, starts with an empty register and never sees
// another customer's data.
test("regisztráció: új fiók saját, üres nyilvántartással", async ({ page }) => {
  const email = `teszt-${Date.now()}@pelda.hu`;
  await page.goto("/");
  await page.getByRole("button", { name: "Kipróbálom 30 napig ingyen" }).click();
  await page.locator("input[name=organization]").fill("Próbafalva Önkormányzata");
  await page.locator("input[name=name]").fill("Teszt Elek");
  await page.locator("input[name=email]").fill(email);
  await page.locator("input[name=password]").fill("nagyon-titkos-1");
  await page.locator("input[name=password2]").fill("nagyon-titkos-1");
  await page.getByRole("button", { name: "Fiók létrehozása" }).click();
  // first sign-in goes to onboarding (no cemetery yet)
  await expect(page.getByLabel("Üzemeltető neve")).toHaveValue("Próbafalva Önkormányzata", { timeout: 15_000 });
  await expect(page.getByLabel("Aláíró neve")).toHaveValue("Teszt Elek");

  // the sample customer's people are not searchable from this account
  const res = await page.evaluate(async () => {
    const r = await fetch("/rpc", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ cmd: "api", args: { method: "search.quick", args: { query: "kovacs", limit: 30 } } }) });
    return r.json();
  });
  expect(res.ok).toEqual([]);

  // logging out ends the session
  await page.evaluate(() => fetch("/rpc", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ cmd: "logout", args: {} }) }));
  const after = await page.evaluate(async () => (await fetch("/rpc", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ cmd: "api", args: { method: "search.quick", args: { query: "a", limit: 1 } } }) })).json());
  expect(after.err.code).toBe("session_expired");
});

test("idegen oldalról érkező kérés elutasítva", async ({ request }) => {
  const r = await request.post("/rpc", { headers: { "Content-Type": "application/json", Origin: "https://tamado.example" }, data: { cmd: "app_status", args: {} } });
  expect(r.status()).toBe(403);
});
