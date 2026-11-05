# A TEMISZ élesítése (Vercel + Neon)

A lépések sorrendje fontos. A `main` ágra csak akkor kerüljön a webes változat, amikor az 1–4. lépés kész, mert a `main` minden feltöltése azonnal élesít.

## 1. Domain

1. Regisztrálja a **temisz.hu** domaint egy magyar regisztrátornál (például Nethely).
2. A Vercelben: Project › Settings › Domains › `temisz.hu` és `www.temisz.hu` hozzáadása.
3. A regisztrátornál állítsa be a Vercel által mutatott DNS-rekordokat (A rekord a gyökérre, CNAME a `www`-re).

## 2. Adatbázis (Neon)

1. Vercel › Storage › Create › **Neon**. Régió: **Frankfurt (eu-central-1)**. Csomag: kezdetben a legkisebb fizetős (a napi mentés és az időponthoz visszaállítás miatt).
2. Kösse a projekthez. A Vercel beállítja a `DATABASE_URL` változót.
3. A TEMISZ a **közvetlen (nem pooled)** kapcsolatot használja, mert ügyfelenként sémát vált: ha van `DATABASE_URL_UNPOOLED`, azt választja, és ezt a Neon-integráció beállítja.
4. A sémát a szerver első indításkor maga hozza létre (platform séma, majd ügyfelenként a saját séma).

## 3. E-mail (Resend)

1. Vercel › Integrations › **Resend**, vagy közvetlenül a resend.com-on.
2. Domain hozzáadása: `temisz.hu`. A megadott SPF-, DKIM- és DMARC-rekordokat vegye fel a DNS-be.
3. Környezeti változók: `RESEND_API_KEY`, `MAIL_FROM=TEMISZ <noreply@temisz.hu>`.

## 4. Környezeti változók (Vercel › Settings › Environment Variables, Production)

| Változó | Érték |
| --- | --- |
| `PUBLIC_URL` | `https://temisz.hu/app` |
| `CRON_SECRET` | hosszú véletlen szöveg (például `openssl rand -hex 32`) |
| `SUPERADMIN_EMAILS` | a TEMISZ üzemeltetőinek e-mail-címei, vesszővel elválasztva |
| `SZAMLAZZ_AGENT_KEY` | Számlázz.hu › Beállítások › Számla Agent kulcs |
| `BLOB_READ_WRITE_TOKEN` | Vercel › Storage › Blob létrehozása után automatikusan |

A Számlázz.hu-fiókban adja meg a bankszámlaszámot és a cégadatokat: a díjbekérőn és a számlán ezek jelennek meg.

**Számlázás ellenőrzése élesítés előtt:** a Számla Agent kérés felépítése a dokumentáció szerint készült, de valódi kulccsal még nem futott. Az első díjbekérőt kísérje figyelemmel: a sorszám, a PDF és az e-mail is megérkezik-e.

## 5. Bankkártyás fizetés (Stripe, nem kötelező)

1. Stripe-fiók, a cégadatokkal és a magyar bankszámlával.
2. Products › Tax rates: **27%, exclusive** adókulcs. Az azonosítója (`txr_…`) lesz a `STRIPE_TAX_RATE_ID`.
3. Developers › Webhooks › Add endpoint: `https://temisz.hu/api/stripe/webhook`. Események:
   - `checkout.session.completed`
   - `invoice.paid`
   - `invoice.payment_failed`
   - `customer.subscription.updated`
   - `customer.subscription.deleted`

   Az aláíró titok lesz a `STRIPE_WEBHOOK_SECRET`.
4. Settings › Billing › Customer portal: engedélyezze a kártyacserét és a lemondást.
5. Változók: `STRIPE_SECRET_KEY` (élesben `sk_live_…`), `STRIPE_WEBHOOK_SECRET`, `STRIPE_TAX_RATE_ID`.

Amíg a `STRIPE_SECRET_KEY` nincs megadva, a programban csak az átutalás választható.

## 6. Élesítés

1. Ellenőrizze egy előnézeti telepítésen (a fejlesztői ág minden feltöltése ilyet készít): regisztráció, megerősítő e-mail, előfizetés átutalással.
2. Töltse fel a `main` ágra. A Vercel lefordítja a felületet, a weboldalt és a Rust-függvényt, és élesít.
3. A napi feladatot a `vercel.json` állítja be (03:17 UTC). A Vercel › Cron Jobs oldalon kézzel is futtatható.

## 7. Jogi szövegek

A `website/aszf.html`, `adatkezeles.html`, `adatfeldolgozas.html` és `impresszum.html` **tervezetek**. Élesítés előtt jogásznak át kell néznie őket, és ki kell tölteni a szögletes zárójeles adatokat (cégnév, székhely, adószám, elérhetőségek).

## Költség induláskor (tájékoztató)

| Szolgáltatás | Havonta |
| --- | --- |
| Vercel Pro | 20 USD |
| Neon (Launch) | 19 USD-től |
| Resend | 0–20 USD |
| Számlázz.hu | a választott csomag szerint |
| Stripe | tranzakciónként (EU-s kártya kb. 1,5% + 0,25 EUR) |
