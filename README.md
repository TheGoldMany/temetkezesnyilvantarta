# Lunda Temető

Temetőnyilvántartó asztali alkalmazás Windowsra és macOS-re. Elektronikusan vezeti a temetőüzemeltetők törvényben előírt nyilvántartásait: a nyilvántartó könyvet és a sírbolt könyvet. Kezeli a temetési helyek teljes életciklusát (megváltás, újraváltás, lejárat, újraértékesítés), a térképet és a nyomtatványokat. Internetkapcsolat nélkül is teljes értékűen működik.

- Specifikáció: [`docs/FRS.md`](docs/FRS.md)
- Megvalósítási terv: [`docs/PLAN.md`](docs/PLAN.md)
- Nyitott kérdések és a választott alapértelmezések: [`docs/QUESTIONS.md`](docs/QUESTIONS.md)
- Felületi irány: [`docs/DESIGN.md`](docs/DESIGN.md)

> A jogszabályi megfelelést (Ttv., Vhr., helyi rendeletek) élesítés előtt jogásszal kell ellenőriztetni. A helyi szabályokat a pilot üzemeltetővel kell egyeztetni (FRS 2. fejezet).

## Tartalom

1. [Telepítés felhasználóknak](#telepítés-felhasználóknak)
2. [Első indítás](#első-indítás)
3. [Fejlesztői környezet](#fejlesztői-környezet)
4. [Tesztek](#tesztek)
5. [Build](#build)
6. [Kiadási folyamat](#kiadási-folyamat)
7. [Amit be kell szerezned és be kell állítanod](#amit-be-kell-szerezned-és-be-kell-állítanod)
8. [Felépítés](#felépítés)
9. [Adatbiztonság](#adatbiztonság)

## Telepítés felhasználóknak

| Rendszer | Fájl | Megjegyzés |
| --- | --- | --- |
| Windows 10/11, 64 bites (Intel/AMD) | `Lunda Temető_x.y.z_x64-setup.exe` vagy `…_x64_hu-HU.msi` | Az MSI csoportházirenddel is telepíthető. |
| Windows 11 ARM | `Lunda Temető_x.y.z_arm64-setup.exe` | |
| macOS 12 vagy újabb (Apple Silicon és Intel) | `Lunda Temető_x.y.z_universal.dmg` | Nyissa meg, és húzza az alkalmazást a Programok mappába. |

Aláíratlan telepítő esetén (amíg a kódaláíró tanúsítványok nincsenek beállítva):

- **Windows:** a SmartScreen figyelmeztet. Ilyenkor: „További információ”, majd „Futtatás mindenképp”.
- **macOS:** jobb gomb az alkalmazáson, „Megnyitás”, majd még egyszer „Megnyitás”.

Az adatok helye:

- Windows: `%APPDATA%\hu.lunda.temeto\`
- macOS: `~/Library/Application Support/hu.lunda.temeto/`

Az adatbázis titkosított (`lunda-temeto.db`). A kulcsa az operációs rendszer kulcstárában van: Windowson a Hitelesítőadat-kezelőben, macOS-en a Kulcskarikában.

## Első indítás

1. **Rendszergazda fiók** létrehozása (név, felhasználónév, legalább 8 karakteres jelszó).
2. **Üzemeltető adatai:** ezek kerülnek a nyomtatványok fejlécébe.
3. **Mentési mappa:** külső meghajtó, hálózati vagy felhőmappa. A program naponta és kilépéskor ide ment. Ha 7 napja nem volt sikeres külső mentés, figyelmeztet.
4. **Helyreállító kulcs:** nyomtassa ki, és tegye el biztonságos helyre. Ezzel lehet a mentést **másik gépen** visszaállítani (lásd `docs/QUESTIONS.md`, Q7).
5. **Mintatemető betöltése** a kipróbáláshoz (kitalált adatok, valódi személyek nélkül), vagy kezdés üres nyilvántartással.

Új gépre költözéskor vagy a kulcstár elvesztésekor a belépés előtti képernyőn a „Visszaállítás mentésből” gombbal lehet visszatölteni az adatokat. Ehhez kell a `.ltbak` mentésfájl és a helyreállító kulcs.

## Fejlesztői környezet

Előfeltételek:

- Rust (stable, 1.80+), Node.js 22, pnpm 10
- Windows: Microsoft C++ Build Tools és WebView2 (Windows 11-en gyárilag megvan)
- macOS: Xcode Command Line Tools (`xcode-select --install`)
- Linux (csak fejlesztéshez és CI-hez): `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`, valamint `perl` és `make` (az OpenSSL fordításához)

Függőségek telepítése:

```bash
pnpm install
```

Az asztali alkalmazás fejlesztői módban (a felület és a Rust-mag újratöltődik):

```bash
pnpm tauri dev
```

Felület böngészőben, a valódi Rust-maggal (gyors felületi munkához, képernyőképekhez, E2E-hez). A `lunda-devserver` ugyanazokat a parancsokat szolgálja ki HTTP-n, csak `127.0.0.1`-en, és sosem kerül a telepítőbe.

```bash
cargo run -p lunda-devserver -- --data .devdata --reset --sample
```

```bash
pnpm dev
```

Ezután nyissa meg a http://localhost:1420 címet. Belépés: `admin` / `admin-jelszo`. A `--large` kapcsoló egy további, 50 000 helyes és 100 000 elhunytas temetőt is generál teljesítményméréshez.

Hasznos parancsok:

| Parancs | Mit csinál |
| --- | --- |
| `cargo test -p lunda-core` | Rust egység- és integrációs tesztek |
| `cargo test -p lunda-core --release --test perf -- --ignored --nocapture` | Teljesítményteszt (100 000 elhunyt, 50 000 hely) |
| `pnpm typecheck` | TypeScript-ellenőrzés |
| `pnpm test` | Felületi egységtesztek (Vitest) |
| `pnpm e2e` | End-to-end tesztek (Playwright) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint |
| `cargo fmt --all` | Formázás |

## Tesztek

- **Üzleti szabályok** (`crates/lunda-core/src/rules.rs`, egységtesztek): lejárat, szökőnap, nyugvási idő és határnapja, rátemetési tiltás és feltétel, kettős sír, urnaférőhely, időarányos díj és visszatérítés, helyállapot-számítás, türelmi idő.
- **Napló hash-lánca** (`audit.rs`): a módosítást és a törlést trigger tiltja, a megkerült manipulációt a lánc-ellenőrzés kimutatja.
- **Adatbázis-integráció** (`tests/register.rs`):
  - folyószám-folytonosság: elutasított temetés nem fogyaszt folyószámot, az érvénytelenítés megtartja, a régi papír folyószám továbbviszi a számlálót, az évenkénti újrakezdés működik;
  - fizikai törlés tiltása, javítás indoklással, jogosultságok;
  - UC-01, UC-02 és UC-03 a szolgáltatási rétegen;
  - sírbolt férőhelyei, jogosultváltás, minden nyomtatvány előállítása.
- **Import és mentés** (`tests/import.rs`, `tests/backup_app.rs`): próbafuttatás nyom nélkül, „minden vagy semmi” elv, régi folyószámok megtartása, export; mentés, ellenőrzés, visszaállítás bejelentkezve és új gépen.
- **Teljesítmény** (`tests/perf.rs`): keresés < 300 ms, a térkép adatai < 1 s 50 000 helynél. A legutóbbi mérés: keresés 39–76 ms, térkép 764 ms.
- **E2E** (`e2e/flows.spec.ts`): UC-01 új temetés, UC-02 rátemetés-tiltás és urna, UC-03 lejárat és értesítés, UC-05 import, gyors bevitel és ellenőrző riport, valamint a hiteles könyv nyomtatása.

Az E2E-tesztek a valódi Rust-magot használják a fejlesztői hídon keresztül, Chromiumban. A natív WebView-t (`tauri-driver`) csak Linuxon és Windowson lehetne vezérelni; a macOS-buildet a kiadási folyamat ellenőrzi.

## Build

Helyi telepítőkészítés az adott gépre (aláírás nélkül):

```bash
pnpm tauri build
```

- Windows: `src-tauri/../target/release/bundle/nsis/*.exe` és `bundle/msi/*.msi`
- macOS universal (mindkét Rust-cél kell):

  ```bash
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  ```

  ```bash
  pnpm tauri build --target universal-apple-darwin
  ```

Windows ARM64 (csak NSIS, mert a WiX MSI nem támogatja az ARM64-et):

```bash
pnpm tauri build --target aarch64-pc-windows-msvc --bundles nsis
```

## Kiadási folyamat

1. Emeld a verziót mindhárom helyen: `package.json`, `src-tauri/tauri.conf.json` (`version`), `Cargo.toml` (`[workspace.package] version`).
2. Commit, majd tag:

   ```bash
   git tag v0.2.0
   ```

   ```bash
   git push origin v0.2.0
   ```

3. A **Release** munkafolyamat (`.github/workflows/release.yml`) lefordítja és aláírja a telepítőket (Windows x64 és ARM64, macOS universal), majd **piszkozat** GitHub-kiadást hoz létre. Ha be van állítva a frissítési kulcs, a kiadásba bekerülnek az aláírt frissítőcsomagok és a `latest.json` is.
4. Ellenőrizd a piszkozatot, próbáld ki a telepítőket, majd tedd közzé.
5. Másold a `latest.json`-t és a frissítőcsomagokat a frissítési szerverre (az `UPDATER_ENDPOINT` által mutatott helyre). Az alkalmazás a Beállítások, „Névjegy és frissítés” fülön keres frissítést. Telepítés előtt automatikus mentést készít, majd ellenőrzi a csomag aláírását.

A **CI** munkafolyamat (`.github/workflows/ci.yml`) minden pushra lefut:

- Rust-tesztek Windowson, macOS-en és Linuxon (a titkosított SQLite mindhárom rendszeren lefordul);
- formázás és lint;
- teljesítményteszt;
- felületi ellenőrzések;
- E2E;
- az asztali héj fordítása.

## Amit be kell szerezned és be kell állítanod

Minden tétel elhagyható: titkok nélkül a folyamat aláíratlan telepítőket készít, és nem lesz automatikus frissítés.

### 1. Tauri frissítési kulcspár (ingyenes, kötelező az automatikus frissítéshez)

```bash
pnpm tauri signer generate -w ~/.tauri/lunda-temeto.key
```

| Hová | Név | Érték |
| --- | --- | --- |
| Secret | `TAURI_SIGNING_PRIVATE_KEY` | a privát kulcs tartalma |
| Secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | a megadott jelszó |
| Variable | `TAURI_UPDATER_PUBKEY` | a kiírt nyilvános kulcs |
| Variable | `UPDATER_ENDPOINT` | pl. `https://updates.lunda.hu/temeto/{{target}}/{{arch}}/{{current_version}}`, vagy egy statikus `latest.json` URL-je |

A privát kulcsot tartsd biztonságban. Ha elveszik, a már telepített példányok nem fogadnak el új frissítést.

### 2. Frissítési szerver

Egy HTTPS-tárhely a `latest.json` és a csomagok számára. Lehet statikus tárhely (pl. S3, Cloudflare R2), vagy maga a GitHub Releases. A cím futás közben a programban is átírható (Beállítások, „Frissítési szerver címe”).

### 3. Windows kódaláíró tanúsítvány (kb. 200–500 EUR/év)

OV vagy EV tanúsítvány egy hitelesítésszolgáltatótól (pl. Sectigo, DigiCert, SSL.com). Az EV tanúsítvány azonnal megszünteti a SmartScreen-figyelmeztetést; az OV-nél ehhez idő kell.

- Fájlos (.pfx) tanúsítványnál:

  | Secret | Érték |
  | --- | --- |
  | `WINDOWS_CERTIFICATE` | a .pfx base64-ben (`certutil -encode cert.pfx cert.txt`, a fejléc- és lábléc-sorok nélkül) |
  | `WINDOWS_CERTIFICATE_PASSWORD` | a .pfx jelszava |
  | `WINDOWS_CERTIFICATE_THUMBPRINT` | a tanúsítvány SHA-1 ujjlenyomata |

- Felhős HSM-nél (EV tanúsítványnál 2023 óta kötelező; pl. Azure Trusted Signing, SSL.com eSigner) a `bundle.windows.signCommand` beállítás kell. Ehhez a `release.yml`-t a szolgáltató leírása szerint kell kiegészíteni.

### 4. Apple Developer Program (99 USD/év)

| Secret | Érték |
| --- | --- |
| `APPLE_CERTIFICATE` | „Developer ID Application” tanúsítvány .p12-ben, base64 (`openssl base64 -A -in cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | a .p12 jelszava |
| `APPLE_SIGNING_IDENTITY` | pl. `Developer ID Application: Lunda Software Kft. (ABCDE12345)` |
| `APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_P8` | App Store Connect API-kulcs a notarizációhoz: Issuer ID, Key ID és a .p8 fájl tartalma |

### 5. Egyéb

- **GitHub:** a Settings, Actions, General oldalon a munkafolyamatoknak kapjanak írási jogot (*Read and write permissions*), hogy létrehozhassák a kiadást.
- **Windows ARM64:** a build a `windows-latest` futtatón keresztfordítással készül. Ha a beépített OpenSSL ARM64-fordítása elakadna, használj `windows-11-arm` futtatót (ha elérhető a csomagodban), és ott a `--target` kapcsoló elhagyható.

## Felépítés

```
crates/lunda-core/      Rust könyvtár: minden üzleti szabály, jogosultság és adat
  migrations/           verziózott SQL-migrációk (soha ne módosíts kiadott migrációt)
  fonts/                beágyazott betűkészletek a PDF-hez (SIL OFL 1.1)
  src/rules.rs          lejárat, nyugvási idő, temethetőség, időarányos díj, állapot
  src/store/            szolgáltatások: szerkezet, elhunytak, események, megváltások, keresés…
  src/audit.rs          hash-láncolt változásnapló
  src/backup.rs         titkosított mentés, ellenőrzés, visszaállítás
  src/pdf/              PDF-motor és nyomtatványok
  src/io.rs             import, export, átállási ellenőrzés
  src/api.rs            egyetlen JSON-diszpécser a felület hívásaihoz
  src/app.rs            munkamenet, inaktivitás, mentési és visszaállítási műveletek
crates/lunda-devserver/ fejlesztői és E2E-híd (HTTP, csak localhost)
src-tauri/              asztali héj: kulcstár, ütemezett mentés, frissítő
src/                    React + TypeScript + Tailwind felület
  i18n/hu.json          az összes felületi szöveg (új nyelv = új fájl)
e2e/                    Playwright end-to-end tesztek
```

## Adatbiztonság

| Terület | Megvalósítás | Hivatkozás |
| --- | --- | --- |
| Adatbázis | SQLCipher (AES-256), WAL-mód, a kulcs a rendszer kulcstárában | NFR-BIZ-01, NFR-TEL-04 |
| Jelszavak | Argon2id; 5 sikertelen belépés után 15 perces zárolás; tétlenség után kijelentkeztetés (beállítható) | NFR-BIZ-02 |
| Változásnapló | Csak hozzáírható, láncolt SHA-256 ellenőrzőösszeggel; a lánc a Beállítások, „Változásnapló” fülön ellenőrizhető | FR-NAP-01 |
| Törlés | Fizikai törlés nincs (adatbázis-trigger tiltja), csak érvénytelenítés indoklással | FR-NAP-02 |
| Napló a hozzáférésekről | Naplózott a személyes adatok megtekintése és exportja | NFR-BIZ-03 |
| Mentések | Helyreállító kulccsal titkosítva, SHA-256 manifeszttel és naplózva | FR-NAP-03–06 |
| Export | A teljes export nem tartalmaz jelszó-hasht és helyreállító kulcsot | FR-IMP-04 |

### Betűkészletek

A PDF-ekbe ágyazott Literata és Atkinson Hyperlegible Next betűkészlet a SIL Open Font License 1.1 alatt áll. A licencek: `crates/lunda-core/fonts/OFL-*.txt`.
