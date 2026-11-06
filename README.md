# TEMISZ

**Temetőmenedzsment és Informatikai Szoftver.** Webes temetőnyilvántartó szolgáltatás önkormányzatoknak, egyházközségeknek és temetőüzemeltetőknek: nyilvántartó és sírbolt könyv, temetési helyek, megváltások és lejáratok, temetőtérkép, hiteles nyomtatás, adatátvétel. Az üzemeltetéshez: pénzügy és házipénztár, számlázás (Számlázz.hu vagy ASP), temetési naptár, munkalapok, vállalkozók, ügyiratok, temetőbejárás, éves beszámoló a fenntartónak, automatikus lejárati emlékeztetők, közös naptár a temetkezési irodákkal, nyilvános sírkereső térképpel. Az első belépéskor bemutató vezet végig a programon, a Súgó oldalon minden feladathoz lépésenkénti leírás van. Böngészőből működik, 30 napig ingyenes, utána temetőnként havi 45 000 Ft + ÁFA (vagy évi 450 000 Ft + ÁFA).

A követelmények: [docs/FRS.md](docs/FRS.md) (nyilvántartás), [docs/FRS-UZEMELTETES.md](docs/FRS-UZEMELTETES.md) (fenntartói és üzemeltetői modulok). A webes átállás terve és állapota: [docs/PLAN-WEB.md](docs/PLAN-WEB.md). A felület egységes dizájnja: [docs/DESIGN.md](docs/DESIGN.md). Élesítés: [DEPLOY.md](DEPLOY.md).

## Tartalom

- [Felépítés](#felépítés)
- [Fejlesztői környezet](#fejlesztői-környezet)
- [Tesztek](#tesztek)
- [Környezeti változók](#környezeti-változók)
- [Adatbiztonság](#adatbiztonság)
- [Az asztali programról](#az-asztali-programról)

## Felépítés

| Rész | Hely | Mit csinál |
| --- | --- | --- |
| Üzleti logika | `crates/lunda-core` | Nyilvántartás, szabályok (rátemetés, nyugvási idő, folyószám), változásnapló (hash-lánc), PDF, Excel/CSV import-export. Postgresre a `sql.rs` rétegen át ír. |
| Webszerver | `crates/temisz-server` | Ügyfelek (bérlők), fiókok, munkamenetek, e-mail, kétlépcsős azonosítás, előfizetés és számlázás, adatexport, a fiók megszüntetése. |
| Vercel-függvény | `api/rpc.rs` (a gyökér `Cargo.toml` csomagja) | Ugyanaz a szerver Vercel-függvényként, a frankfurti régióban. |
| Asztali mentés betöltése | `crates/temisz-desktop-import` | A régi asztali TEMISZ SQLCipher-mentéseit olvassa a helyreállító kulccsal. |
| Felület | `src/` | React 19, TanStack Query, Tailwind v4, magyar szövegek a `src/i18n/hu.json`-ban. |
| Weboldal | `website/` | Statikus nyitóoldal, árak, jogi szövegek, a beágyazható sírkereső (`kereso.html`). Élesben a gyökérben, a program a `/app/` alatt. |

**Adatbázis:** Postgres (élesben Neon, Frankfurt). Minden ügyfél saját sémát kap (`t_<azonosító>`), a kapcsolat `search_path`-ja mindig az aktuális ügyfélre mutat, így egy lekérdezés sem érhet el más ügyfél adatához. A központi adatok (ügyfelek, fiókok, munkamenetek, előfizetések, számlák) a `platform` sémában vannak.

**Kommunikáció:** a felület `POST /rpc` kérésekkel beszél a szerverrel (`{cmd, args}`). A munkamenet HttpOnly süti; csak azonos eredetű JSON-kérést fogad el a szerver. Egyetlen nyilvános végpont van: `GET /api/public/search?t=<token>&q=<név>`, a sírkereső. Csak akkor ad találatot, ha az üzemeltető közzétette, és percenként 60 kérésre korlátozott.

## Fejlesztői környezet

Szükséges: Rust (stable), Node 22, pnpm 10, Postgres 16 (ICU-val, `pg_trgm` bővítménnyel).

```bash
pnpm install
```

```bash
export DATABASE_URL=postgres://postgres@127.0.0.1:5433/postgres
```

```bash
pnpm build && cargo run -p temisz-server -- --port 1430 --static dist --insecure-cookies --dev --sample-account
```

Ezután a http://127.0.0.1:1430 címen a mintafiók: `admin@minta.temisz.hu` / `admin-jelszo`. Fejlesztői módban (`--dev`):

- az e-maileket nem küldi el, csak naplózza (`/dev/outbox?to=…`);
- a próbaidő lejártatható (`POST /dev/expire-trial?email=…`);
- a napi feladat kézzel futtatható (`/api/cron/daily`);
- a belépési korlátok ki vannak kapcsolva.

Felület gyors újratöltéssel: `pnpm dev` (a Vite a `/rpc` kéréseket a 1430-as portra továbbítja).

Az élesítéshez hasonló összerakás: `pnpm build:web` (weboldal + `public/app`), majd `--static public`.

## Tesztek

```bash
cargo test --workspace
```

```bash
pnpm typecheck && pnpm test
```

```bash
pnpm e2e
```

- **Rust:** szabályok, változásnapló, import, Postgres-réteg, ügyfelek elkülönítése, TOTP, Számlázz.hu XML, banki befizetések párosítása, adatexport és asztali mentés betöltése.
- **E2E:** valódi böngésző, szerver és Postgres. Lefedi a temetés rögzítését, a rátemetést, a lejáratokat, az importot és a hiteles nyomtatást. Ugyanígy a regisztrációt, a meghívót, az elfelejtett jelszót, a kétlépcsős belépést, az előfizetést, a csak olvasható módot és a fiók megszüntetését. Az üzemeltetésből: számla, készpénzes befizetés és pénztárzárás; foglalás ütközéssel, ismétlődő munkalap, iktatás és döntés, bejárás; éves beszámoló, e-mail értesítő és a nyilvános sírkereső.
- **Teljesítmény:** 100 000 elhunyt és 50 000 sírhely mellett a keresés 300 ms alatt fut, futtatása: `cargo test -p lunda-core --release --test perf -- --ignored --nocapture`.

## Környezeti változók

| Változó | Kötelező | Mire jó |
| --- | --- | --- |
| `DATABASE_URL` | igen | Postgres (Neonnál a közvetlen, nem pooled cím, `sslmode=require`) |
| `PUBLIC_URL` | igen | A program címe a levelekben, pl. `https://temisz.hu/app` |
| `CRON_SECRET` | igen | A napi feladat hívásának titka (a Vercel Cron automatikusan küldi) |
| `RESEND_API_KEY`, `MAIL_FROM` | igen | E-mail küldés (pl. `TEMISZ <noreply@temisz.hu>`) |
| `SUPERADMIN_EMAILS` | igen | A TEMISZ üzemeltetőinek e-mail-címei, vesszővel elválasztva (Ügyfelek oldal) |
| `SZAMLAZZ_AGENT_KEY` | élesben | Számlázz.hu Számla Agent kulcs; nélküle TESZT-sorszámú bizonylatok készülnek |
| `BLOB_READ_WRITE_TOKEN` | ajánlott | Napi teljes export a Vercel Blobba |

## Adatbiztonság

- Ügyfelenként külön séma, a sémaváltást teszt ellenőrzi.
- Változásnapló SHA-256 hash-lánccal. Törlés és utólagos módosítás helyett érvénytelenítés, ezt adatbázis-triggerek kényszerítik ki.
- Argon2id jelszókivonat, kizárás túl sok hibás próbálkozás után, kétlépcsős azonosítás (a rendszergazdáknak kötelező).
- HttpOnly, `Secure`, `SameSite=Lax` süti, azonos eredetű kérések, HSTS, korlátozott belépési és regisztrációs próbálkozások.
- Mentés: a Neon időponthoz visszaállítható mentése és napi teljes export. Az ügyfél bármikor letöltheti az összes adatát.

## Az asztali programról

A 0.1.2-es asztali (Windows, macOS) változat lezárult; a forrása a `v0.1.2` tagben megtalálható. A mentései a webes fiókba betölthetők (Beállítások › Mentés és visszaállítás).
