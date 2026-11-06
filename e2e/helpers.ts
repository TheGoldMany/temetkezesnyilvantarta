import { createHmac } from "node:crypto";
import { expect, type Page } from "@playwright/test";

export async function login(page: Page) {
  await page.goto("/");
  const user = page.locator("input[name=email]");
  await page.locator('input[name=email], nav[aria-label="Fő menü"]').first().waitFor({ timeout: 30_000 });
  if (await user.isVisible()) {
    await user.fill("admin@minta.temisz.hu");
    await page.locator("input[name=password]").fill("admin-jelszo");
    await page.getByRole("button", { name: "Belépés" }).click();
  }
  await expect(page.getByRole("navigation", { name: "Fő menü" })).toBeVisible();
}

export async function nav(page: Page, label: string) {
  await page.getByRole("navigation", { name: "Fő menü" }).getByRole("button", { name: label, exact: true }).click();
}

/** Last e-mail sent to an address (dev server log at /dev/outbox). */
export async function lastMail(page: Page, to: string, kind?: string): Promise<{ subject: string; body: string; kind: string }> {
  let mails: { subject: string; body: string; kind: string }[] = [];
  await expect
    .poll(async () => {
      mails = await (await page.request.get(`/dev/outbox?to=${encodeURIComponent(to)}`)).json();
      return mails.filter((m) => !kind || m.kind === kind).length;
    })
    .toBeGreaterThan(0);
  return mails.filter((m) => !kind || m.kind === kind)[0];
}

/** The app link (http://host/#…) inside an e-mail body, as a path. */
export function linkIn(body: string): string {
  const m = body.match(/https?:\/\/[^/\s]+(\/#\S+)/);
  if (!m) throw new Error("no link in e-mail");
  return m[1];
}

/** RFC 6238 code from a base32 secret, like an authenticator app. */
export function totp(secretB32: string, at = Date.now()): string {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let bits = "";
  for (const c of secretB32.replace(/\s/g, "").toUpperCase()) bits += alphabet.indexOf(c).toString(2).padStart(5, "0");
  const key = Buffer.from(bits.match(/.{8}/g)!.map((b) => parseInt(b, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(at / 1000 / 30)));
  const h = createHmac("sha1", key).update(counter).digest();
  const off = h[19] & 0xf;
  const bin = ((h[off] & 0x7f) << 24) | (h[off + 1] << 16) | (h[off + 2] << 8) | h[off + 3];
  return String(bin % 1_000_000).padStart(6, "0");
}

/**
 * Clicks a button that opens a generated PDF in a new tab and returns the
 * first bytes of the document. Reads the blob in the page itself: headless
 * Chromium on CI does not always report the address of a PDF tab.
 */
export async function openedPdf(page: Page, click: () => Promise<void>): Promise<string> {
  await page.evaluate(() => {
    const w = window as unknown as { __opened?: string[]; __openPatched?: boolean };
    w.__opened = [];
    if (!w.__openPatched) {
      const orig = window.open.bind(window);
      window.open = (url?: string | URL, ...rest: unknown[]) => {
        w.__opened!.push(String(url ?? ""));
        return orig(url, ...(rest as [string?, string?]));
      };
      w.__openPatched = true;
    }
  });
  await click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __opened: string[] }).__opened.join(" "))).toContain("blob:");
  const head = await page.evaluate(async () => {
    const url = (window as unknown as { __opened: string[] }).__opened[0];
    const buf = await (await fetch(url)).arrayBuffer();
    return new TextDecoder().decode(buf.slice(0, 5));
  });
  for (const p of page.context().pages()) if (p !== page) await p.close();
  return head;
}
