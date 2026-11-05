# Lunda Temető – MVP megvalósítási terv

Állapot: **jóváhagyva, M1–M8 elkészült** (lásd a 9. fejezetet) · Alap: `docs/FRS.md` v0.1 (8.1 „MVP” sor + a megbízásban felsorolt 13 pont) · Nyitott kérdések: `docs/QUESTIONS.md`

## 1. Architektúra

```
lunda-temeto/
├─ crates/lunda-core/     Tiszta Rust könyvtár, Tauri nélkül tesztelhető:
│   ├─ db/                SQLCipher-kapcsolat, WAL, migrációk (PRAGMA user_version), HU kolláció
│   ├─ domain/            entitások, állapotgép, validáció
│   ├─ rules/             folyószám, lejárat, nyugvási idő, temethetőség, időarányos díj
│   ├─ audit/             hash-láncolt napló + ellenőrzés
│   ├─ search/            FTS5 + normalizált (ékezet nélküli, kisbetűs) oszlopok
│   ├─ pdf/               nyilvántartó könyv, sírbolt könyv, nyomtatványok, térkép-PDF
│   ├─ io/                Excel/CSV import (próbafuttatás) és export (CSV/XLSX/JSON)
│   ├─ backup/            titkosított mentés, SHA-256 manifeszt, ellenőrzés, visszaállítás
│   └─ auth/              felhasználók, szerepkörök, Argon2id, zárolás
├─ src-tauri/             Vékony réteg: Tauri-parancsok → lunda-core; keyring; ütemező; updater
├─ src/                   React + TS + Vite + Tailwind felület
│   ├─ i18n/hu.json       minden felületi szöveg (i18next)
│   └─ features/…         képernyők a bal oldali menü szerint
└─ .github/workflows/     CI (teszt) + release (Windows x64/ARM64, macOS universal)
```

Elv: **minden üzleti szabály és jogosultság-ellenőrzés a Rust oldalon** fut; a felület csak megjelenít és parancsot küld. Így a szabályok egységtesztelhetők, és a felület nem tudja megkerülni őket.

## 2. Technológiai döntések

| Terület | Választás | Indok |
| --- | --- | --- |
| Adatbázis | `rusqlite` + `bundled-sqlcipher-vendored-openssl` | AES-256, rendszer-OpenSSL nélkül buildel Windowson és macOS-en |
| Migrációk | saját, verziózott SQL-fájlok (`migrations/0001_*.sql`), `user_version` | egyszerű, átlátható; migráció előtt automatikus mentés (NFR-KAR-01) |
| Kulcs | `keyring` crate (Windows Credential Manager, macOS Keychain) | NFR-BIZ-01; fejlesztői/Linux CI-hez fájlalapú tartalék csak teszt-feature mögött |
| Jelszó | `argon2` (Argon2id) | NFR-BIZ-02 |
| Hash | `sha2` (SHA-256) | napló-lánc, mentés-ellenőrzőösszeg |
| Keresés | SQLite FTS5 (`unicode61 remove_diacritics 2`) + prefix-keresés | 100 000 elhunytnál is jóval 300 ms alatt |
| Magyar rendezés | saját `HU` SQLite-kolláció Rustban (cs, dz, dzs, gy, ly, ny, sz, ty, zs, hosszú magánhangzók) + `Intl.Collator('hu')` a felületen | NFR-HAS-01 |
| PDF | `krilla` (beágyazott, szabad licencű betűkészlet, pl. Noto Sans/Serif) – tartalék: `printpdf` | ékezethelyes szöveg, vektoros térkép A4–A0 |
| Excel/CSV | `calamine` (olvasás), `rust_xlsxwriter` (írás), `csv` | FR-IMP-01/04 |
| Felület | React 19, TypeScript, Vite, Tailwind v4, TanStack Query/Virtual/Table, react-hook-form + zod, i18next | |
| Térkép | SVG, saját nézetablak-kivágás (quadtree) + részletességi szintek | 50 000 helynél is gördülékeny |
| Tauri pluginok | updater, dialog, fs, process, single-instance | |

A konkrét API-kat (Tauri 2, rusqlite/SQLCipher, keyring, krilla, Tailwind v4) minden mérföldkő elején a Context7-tel ellenőrzöm.

## 3. Kulcsfontosságú megoldások

- **Folyószám (FR-ELH-03):** `register_sequence(scope_key, next_value)` tábla; kiosztás ugyanabban az `IMMEDIATE` tranzakcióban, mint az esemény beszúrása → nincs kihagyás, nincs ütközés. `UNIQUE(scope, folio)` + trigger tiltja a folyószám módosítását. Hatókör beállítható: temetőnként folyamatos (alapértelmezés) vagy temetőnként és évenként. Gyors bevitelnél (FR-IMP-03) régi folyószám megadható, de csak „történeti” jelöléssel és ütközésellenőrzéssel.
- **Nem törölhetőség (FR-NAP-02, JOG-04):** nincs `DELETE` a parancsok között; adatbázis-triggerek tiltják a fizikai törlést a nyilvántartási táblákon és bármilyen módosítást az `audit_log`-on. Érvénytelenítés = `voided_at`, `voided_by`, `void_reason`; a bejegyzés látható marad (áthúzva).
- **Napló (FR-NAP-01):** `hash = SHA256(prev_hash ‖ kanonikus JSON(időpont, felhasználó, entitás, művelet, régi, új, indoklás))`. Ellenőrző funkció a Beállításokban és minden mentés előtt; törés esetén figyelmeztetés.
- **Temethetőség (FR-ELH-04, UC-02):** egy tiszta függvény (`can_inter(plot, events, restrictions, date) -> Verdict{allowed, reasons[]}`), amely ellenőrzi: helyállapot, férőhely (koporsó/urna külön), nyugvási idő a legutóbbi koporsós temetéstől, temetési tilalom/korlátozás. Tiltásnál a felület megállít és megindokol; szükség esetén csak „javítás” jogú felhasználó bírálhatja felül indoklással (naplózva).
- **Lejárat és újraváltás (FR-MEG-01, 03, 04):** lejárat = kezdet + időtartam (naptári; febr. 29. → febr. 28.). Rátemetésnél új lejárat = max(jelenlegi lejárat, temetés napja + nyugvási idő), díj = (hosszabbítás napjai / teljes időtartam napjai) × aktuális díjtétel. Időarányos visszatérítés a megváltáskori díjból. Pénz egész forintban (`i64`), kerekítés a helyi szabály szerint (alapértelmezés: egész forintra).
- **Állapotgép (FR-TEM-05):** a hely állapota számított (megváltások + események + értesítések), kézi felülírás csak indoklással. Az „újraértékesíthető” állapot csak a türelmi idő után, megerősítéssel.
- **Térkép (FR-TER):** a geometria az adatbázisban (helyenként x, y, szélesség, magasság, forgatás). A felület egyszerre csak a nézetablakban látható helyeket rajzolja ki; kis nagyításnál parcella-szintű összesítő téglalapok. Szerkesztés: parcella téglalap, sorok/helyek automatikus kiosztása, mozgatás, forgatás, két hely egyesítése (kettős sír). PDF-export ugyanabból a geometriából Rustban, A4–A0, jelmagyarázattal.
- **Hiteles könyvek (FR-NYO-01):** folyószám-sorrend, időszak-szűrés, „X. oldal / Y” számozás, hitelesítési záradék és aláírási hely; a láblécben a nyomtatás azonosítója és az adattartalom SHA-256 kivonata. Minden hiteles nyomtatás naplózva.
- **Mentés (FR-NAP-03–06):** `VACUUM INTO` / backup API egy titkosított fájlba + `manifest.json` (SHA-256, időpont, sémaverzió, alkalmazásverzió). Naponta (első indításkor, ha >24 óra telt el, majd időzítővel) és kilépéskor; a legutóbbi N megőrzése. A mentés **külön mentési jelszóval/helyreállító kulccsal** titkosít, hogy gépcsere vagy kulcstárvesztés után is visszaállítható legyen (lásd QUESTIONS). Visszaállítás varázslóval, előtte a jelenlegi állapot mentése. 7 nap sikeres külső mentés nélkül sárga sáv a Kezdőlapon.
- **Jogosultság (3. fejezet):** szerepkör → jogosultság-készlet, egyéni jogosultságok; minden parancs a Rust oldalon ellenőrzi. 5 sikertelen belépés → zárolás (rendszergazda oldja fel, vagy 15 perc). Inaktivitási kijelentkezés (alap: 15 perc). Az Olvasó nem látja a lakcímet/telefonszámot; a személyes adatok megtekintése naplózott (NFR-BIZ-03).

## 4. Felület

Előbb a `frontend-design` skill-lel kialakított design-rendszer: visszafogott, meleg semleges paletta (palakék/zsályazöld akcentus), serif címsorok + jól olvasható sans törzsszöveg, alap 17 px betűméret, nagy kattintási felületek, látható fókuszjelölés, WCAG AA kontraszt világos és sötét témában, nagyítás (90–150%).

Menü: Kezdőlap · Temetések · Temetési helyek · Térkép · Személyek · Lejáratok · Nyomtatványok · Riportok (MVP: kihasználtság, éves statisztika) · Import/Export · Beállítások. Gyorskereső minden képernyő tetején (`Ctrl+K`). Leggyakoribb műveletek (új temetés, rátemetés, keresés + felvilágosítás) legfeljebb 3 kattintás. Első indítás: varázsló (üzemeltető adatai → rendszergazda fiók → mentési hely → mintaadatbázis betöltése vagy üres kezdés).

## 5. Mérföldkövek

Minden mérföldkő végén: `cargo test`, `cargo clippy`, `pnpm test`/`tsc`, rövid összefoglaló (kész / nyitott), commit és push a `claude/lunda-cemetery-mvp-fhiupm` ágra.

| # | Mérföldkő | Tartalom | Tesztek |
| --- | --- | --- | --- |
| M1 | Adatbázis és üzleti logika | `lunda-core`: séma + migrációk, SQLCipher, HU kolláció, domain, szabályok, napló, auth | egységtesztek: folyószám-folytonosság (párhuzamos is), lejárat, nyugvási idő, rátemetés tiltása, időarányos díj/visszatérítés, napló hash-lánca és manipuláció-észlelés, Argon2/zárolás; DB-integrációs tesztek (titkosítás nélkül nem nyitható, triggerek tiltják a törlést) |
| M2 | Alap felület és CRUD | Tauri váz, keyring, belépés, design-rendszer, i18n, temetők/parcellák/sorok/helyek (tömeges létrehozás), elhunytak és események, személyek és jogosultváltás, megváltás, sírbolt könyv férőhelyekkel, beállítások, első indítás varázsló, mintaadat-generátor | komponens-tesztek (Vitest), parancs-szintű integrációs tesztek |
| M3 | Keresés és lejáratok | FTS5 gyorskereső, összetett szűrés, lejáratfigyelő (30/90/365 nap, lejárt), értesítés tényének rögzítése, „újraértékesíthető” megerősítéssel | **teljesítményteszt**: 100 000 elhunyt + 50 000 hely generálva, keresés < 300 ms (CI-ben mérve) |
| M4 | Térkép | SVG-térkép virtualizálással, színkódok + jelmagyarázat, kattintásra adatlap, adatlap ↔ térkép ugrás, rácsszerkesztő | 50 000 helyes renderelési mérés |
| M5 | Nyomtatás | hiteles nyilvántartó és sírbolt könyv, felvilágosítás (térképrészlettel, személyes adat nélkül), megváltási/újraváltási igazolás, lejárati értesítő, sírcímke QR-kóddal, urna-átvételi elismervény, díjbekérő, síremlék-engedély, betűsoros névmutató, térkép-PDF A4–A0, ellenőrzési export | PDF-tartalom tesztek (oldalszám, záradék, sorrend) |
| M6 | Import/export | Excel/CSV varázsló: oszlophozzárendelés, előnézet, hibalista, próbafuttatás (tranzakció visszagörgetéssel); gyors bevitel űrlap (végig billentyűzettel, régi folyószám); teljes export CSV/XLSX/JSON; átállási ellenőrző riport (UC-05) | import-tesztek hibás adatokkal |
| M7 | Mentés és napló felület | ütemezett és kilépéskori mentés, ellenőrzés, visszaállító varázsló, 7 napos figyelmeztetés, naplóböngésző és lánc-ellenőrzés | mentés → sérülés → észlelés; visszaállítás |
| M8 | Telepítők és kiadás | GitHub Actions: CI + release (Windows NSIS x64 és ARM64, MSI x64; macOS universal .dmg), kódaláírás titkos változókból, updater aláírt csomagokkal, konfigurálható frissítési URL; magyar README | **E2E** (WebdriverIO + `tauri-driver`, Linux CI-ben xvfb alatt): UC-01, UC-02, UC-03, UC-05 |

Megjegyzés: a `tauri-driver` macOS-en nem támogatott, ezért az E2E-tesztek Linuxon (és opcionálisan Windowson) futnak; a macOS-buildet a release-folyamat ellenőrzi. Ebben a fejlesztői környezetben a WebKitGTK-csomagokat telepíteni kell az E2E-futtatáshoz; ha ez nem sikerül, az E2E-t a CI-ben futtatom és ezt jelzem.

## 6. Amit neked kell beszerezned (M8-ban részletesen dokumentálom)

| Mit | Mire | GitHub secret |
| --- | --- | --- |
| Windows kódaláíró tanúsítvány (OV/EV; EV esetén felhős HSM, pl. Azure Trusted Signing vagy SSL.com eSigner) | „ismeretlen kiadó” figyelmeztetés elkerülése | `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` (vagy az Azure Trusted Signing változói) |
| Apple Developer Program tagság (99 USD/év), „Developer ID Application” tanúsítvány | macOS aláírás | `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` |
| App Store Connect API-kulcs vagy app-specifikus jelszó | notarizáció | `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH` (vagy `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) |
| Tauri updater kulcspár (`pnpm tauri signer generate`) | frissítőcsomagok aláírása | `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; a nyilvános kulcs a konfigurációba |
| Frissítési szerver (statikus tárhely a `latest.json`-nak) | automatikus frissítés | `UPDATER_ENDPOINT` (repository variable) |

Aláírás nélkül is buildel a folyamat (aláíratlan telepítőkkel), így a titkok beállítása nem blokkolja a fejlesztést.

## 7. Hatókörön kívül az MVP-ben

Befizetések teljes kezelése (FR-DIJ-02–04), sablonszerkesztő, csatolmányok, licenc, kipróbálási időszak, háttérkép a térképhez, duplikátum-összevonás, online sírkereső, többfelhasználós szinkron. Az adatmodell ezeket előkészíti (táblák, mezők), de felület nem készül hozzájuk. A megváltáshoz egy egyszerű, érvényességi idős díjtétel-táblázat készül (helytípusonként), mert a megváltási díj kiszámításához kell.

## 8. Kockázatok

- **Méret és idő:** az MVP nagy; ha egy mérföldkő elhúzódik, előbb a jogszabályi magot (folyószám, könyvek, napló, mentés) fejezem be, és a kényelmi funkciókat jelzem nyitottként.
- **Jogi megfelelés:** a szabályok paraméterezhetők, de a nyomtatott könyv formáját és a hiteles mentés elégségességét jogásznak kell jóváhagynia (FRS 2. fejezet).
- **Kódaláírás és ARM64 Windows:** a WiX (MSI) nem támogatja az ARM64-et, ezért ARM64-re NSIS-telepítő készül.

## 9. Állapot és nyitott tételek (MVP vége)

| Mérföldkő | Állapot |
| --- | --- |
| M1 Adatbázis és üzleti logika | kész |
| M2 Felület és CRUD | kész |
| M3 Keresés, lejáratok, teljesítmény | kész – keresés 39–76 ms, térkép-adat 0,76 s 50 000 helynél |
| M4 Térkép | kész |
| M5 Nyomtatás | kész |
| M6 Import/export, gyors bevitel | kész |
| M7 Mentés, visszaállítás, napló | kész |
| M8 Telepítők, CI, frissítő, E2E, README | kész – az aláírás és a notarizáció a titkok beállításáig nem próbálható ki |

Nyitott, illetve ellenőrzésre váró tételek:

- **Valódi Windows/macOS gépen még nem futott a kész alkalmazás.** A CI mindhárom rendszeren lefordítja és teszteli a Rust-magot. A natív héjat ebben a környezetben Linuxon építettem és indítottam el.
- **Windows ARM64:** keresztfordítással készül. A beépített OpenSSL ARM64-fordítása csak az első kiadási futáskor derül ki (alternatíva: `windows-11-arm` futtató).
- **Térkép 50 000 hellyel:** a teljes adatkészlet 0,76 s alatt áll elő, a böngészőben az első megjelenés kb. 1,3 s. Lassabb irodai gépen ez 1 s fölé mehet. További gyorsítás: az aktív elhelyezések tárolt jelzője vagy a térképadat gyorsítótárazása.
- **E2E:** Chromiumban, a valódi Rust-magon fut (fejlesztői híd). A natív WebView-t vezérlő `tauri-driver` nem került be, mert macOS-en nem támogatott.
- **Az MVP-n kívül maradt (FRS 8.1 szerint v1.0):**
  - befizetések teljes kezelése (FR-DIJ-02–04), sablonszerkesztő (FR-NYO-03), csatolmányok (FR-ELH-07; a táblája már létezik);
  - befogadó nyilatkozat (FR-ELH-06; a táblája már létezik);
  - licenc és kipróbálási időszak (FR-BEA-02), háttérkép a térképhez (FR-TER-02), duplikátumok összevonása (FR-UGY-04), e-mail küldés.
- **Jogi ellenőrzés szükséges:** a hiteles könyv formája és záradéka, a hiteles mentés elégségessége (Q8), valamint a QUESTIONS.md alapértelmezései.
