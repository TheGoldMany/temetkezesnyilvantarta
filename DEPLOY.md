# A TEMISZ élesítése (Vercel + Neon)

A lépések sorrendje fontos. A `main` ágra csak akkor kerüljön a webes változat, amikor az 1–4. lépés kész, mert a `main` minden feltöltése azonnal élesít.

## 1. Domain

1. Regisztrálja a **temisz.hu** domaint egy magyar regisztrátornál (például Nethely).
2. A Vercelben: Project › Settings › Domains › `temisz.hu` és `www.temisz.hu` hozzáadása.
3. A regisztrátornál állítsa be a Vercel által mutatott DNS-rekordokat (A rekord a gyökérre, CNAME a `www`-re).

## 2. Adatbázis (Neon)

1. Vercel › Integrations › **Neon** (vagy Storage › Create › Neon). Régió: **Frankfurt, Germany (West)**. Az **Auth** és a **Realtime** kapcsolója legyen kikapcsolva (a TEMISZ-nek saját belépése van). Csomag: induláskor a Free elég; fizetős ügyfeleknél a napi mentés, az időponthoz visszaállítás és a folyamatosan ébren lévő adatbázis miatt fizetős csomag ajánlott.
2. Connect a Project: a `temetkezesnyilvantarta` projekt, **Production és Preview** környezet. Database branch: a **Preview** legyen bejelölve (minden előnézet saját adatbázis-másolatot kap), a Production ne.
3. **Custom Prefix: `DATABASE`** (nem az alapértelmezett `STORAGE`). Így jön létre a `DATABASE_URL` és a `DATABASE_URL_UNPOOLED`. A TEMISZ a **közvetlen (nem pooled)** kapcsolatot használja, mert ügyfelenként sémát vált, ezért a `DATABASE_URL_UNPOOLED`-et választja. A változók csak az ezután készülő telepítésekben élnek: a bekötés után telepítsen újra.
4. A sémát a szerver első indításkor maga hozza létre (platform séma, majd ügyfelenként a saját séma).

## 3. E-mail (Resend)

1. Vercel › Integrations › **Resend**, vagy közvetlenül a resend.com-on.
2. Domain hozzáadása: `temisz.hu`. A megadott SPF-, DKIM- és DMARC-rekordokat vegye fel a DNS-be.
3. Környezeti változók: `RESEND_API_KEY`, `MAIL_FROM=TEMISZ <noreply@temisz.hu>`.
4. Az ügyfelek nevében küldött levelek (lejárati értesítő, fizetési emlékeztető) is erről a címről mennek; a válasz az üzemeltető Beállítások › Üzemeltető alatt megadott e-mail-címére érkezik (Reply-To). Ezért a temisz.hu domain SPF- és DKIM-rekordját a Resend útmutatója szerint be kell állítani.

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

## 5. Befizetések

Az ügyfelek átutalással fizetnek, díjbekérő alapján (a közleményben az ügyfélkód szerepel). A TEMISZ bankszámlájának kivonatát (CSV vagy Excel a netbankból) az Ügyfelek oldalon kell betölteni: a közlemény és az összeg alapján a program kiegyenlíti a díjbekérőket és kiállítja a számlákat, a bizonytalan tételekre javaslatot ad.

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
