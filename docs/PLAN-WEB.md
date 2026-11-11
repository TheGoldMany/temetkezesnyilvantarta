# TEMISZ webes szolgáltatás – terv

**Döntések (2026. november 5.):**

- **Ár:** létrehozott temetőnként havi 45 000 Ft + ÁFA, vagy éves 450 000 Ft + ÁFA temetőnként.
- **Fizetés:** átutalással, díjbekérő alapján; a befizetéseket a bankszámlakivonatból párosítja a program. (A bankkártyás fizetés kimaradt; ha kell, Barionnal.)
- **Számlázás:** Számlázz.hu.
- **Domain:** temisz.hu.
- **Tárhely:** Vercel, később esetleg Nethely.
- **Funkciók:** a fenntartó és az üzemeltető minden ügye a programban intézhető, a temetkezési pénzügyekkel együtt (lásd [FRS-UZEMELTETES.md](FRS-UZEMELTETES.md)).

**Alapdöntés:** a TEMISZ böngészőből használható előfizetéses szolgáltatás lesz. Minden új ügyfél 30 napot kap ingyen, utána egységes havidíjat fizet. A letölthető program a 0.1.2-es verzióval lezárul. Aki már használja, át tudja hozni az adatait a webes változatba.

## 1. Mi marad és mi változik

A mostani kód nagy része változatlanul átvihető.

| Rész | Webes változatban |
| --- | --- |
| Felület (React, új dizájn) | Marad, kisebb kiegészítésekkel (regisztráció, előfizetés, telefonos nézet). |
| Üzleti logika (`lunda-core`): folyószám, rátemetés-ellenőrzés, megváltás, napló, PDF, import/export | Marad, a szerveren fut. |
| Adatbázis (titkosított SQLite) | Postgresre (Neon) kerül, mert a Vercel nem tárol fájlt. Az adatmodell és a szabályok (folyószám, napló, törlésvédelem) változatlanok. |
| JSON API (`api::dispatch`) | Marad, HTTPS-en, Rust Vercel-függvényként. A fejlesztői szerver (`lunda-devserver`) már most is így működik. |
| Bejelentkezés (Argon2id, zárolás, időkorlát) | Marad, kiegészül e-mail-megerősítéssel, jelszó-visszaállítással és kétlépcsős azonosítással a rendszergazdáknak. |
| Asztali keret (Tauri), frissítő, helyi mentés, mentésre figyelmeztetés | Megszűnik. Helyette szerveroldali mentés és „Minden adat letöltése” gomb. |
| Internet nélküli működés | Megszűnik. Ezt a weboldalon és az ÁSZF-ben is jelezni kell. |

## 2. Felépítés (Vercel)

A Vercel függvényei nem tárolnak fájlt, ezért a helyi SQLite-adatbázis ott nem használható. A felépítés:

- **Felület:** a mostani React-alkalmazás statikus oldalként, a Vercel CDN-jéről.
- **API:** Rust Vercel-függvény (`vercel_runtime`) a frankfurti régióban (`fra1`). Ugyanazt a `lunda-core` üzleti logikát és ugyanazt a JSON API-t futtatja, mint ma.
- **Adatbázis:** Neon Postgres a Vercel Marketplace-ről, frankfurti régióban. A tárolóréteget (`store`) SQLite-ról Postgresre kell átírni:
  - a triggerek PL/pgSQL-ben;
  - a magyar rendezés ICU-kollációval (`hu-HU-x-icu`);
  - a gyorskeresés `pg_trgm`-mel;
  - a migrációk verziózott SQL-fájlokkal.

  Ez a W1 legnagyobb része. Később bármelyik Postgres-szolgáltatóra, Nethelyre is, egyszerű mentés-visszatöltéssel átvihető.
- **Ügyfelek elkülönítése:** minden táblában ügyfél-azonosító (`tenant_id`) és Postgres sorszintű biztonság (RLS). Egy lekérdezési hiba sem adhat ki más ügyfélhez tartozó adatot.
- **Fájlok** (csatolt dokumentumok, fotók, importált táblázatok): Vercel Blob, privát tárolással.
- **Időzített feladatok** (próbaidő-emlékeztetők, lejárati értesítők, napi export): Vercel Cron.
- **E-mail:** tranzakciós e-mail-szolgáltató (például Resend a Marketplace-ről) a temisz.hu domainről küldve.
- **Mentés:** a Neon időponthoz visszaállítható mentése (point-in-time restore), napi titkosított export Blobba, havi visszaállítási próba.
- **Domain:** a temisz.hu-t magyar regisztrátornál kell megvenni (például Nethely), majd a DNS-t a Vercelre irányítani.

Költség induláskor: Vercel Pro (20 USD/hó), Neon (0–19 USD/hó), Resend (0–20 USD/hó). Kezdetben ez havi 10–20 ezer forint körül marad.

## 3. Próbaidő és előfizetés

1. **Regisztráció** a weboldalon: szervezet neve, adószám (nem kötelező), e-mail, jelszó. E-mail-megerősítés után a fiók azonnal használható. Bankkártya nem kell.
2. **30 nap teljes hozzáférés.** A mintatemető egy kattintással betölthető. A 23. és a 28. napon e-mail-emlékeztető megy.
3. **Lejárat után, ha nincs előfizetés:** a fiók csak olvasható lesz, az adatok letölthetők. 60 nap után előzetes értesítés, majd törlés.
4. **Előfizetés:** temetőnként havi 45 000 Ft + ÁFA, vagy éves 450 000 Ft + ÁFA. A díj a fiókban létrehozott, aktív temetők számától függ, a felhasználók száma nem számít. Új temető felvételekor a díj időarányosan nő. A program a temető létrehozása előtt megmutatja, mennyivel. Két fizetési mód:
   - átutalás díjbekérő alapján, havi vagy éves díjjal. Önkormányzatoknál és plébániáknál ez a szokásos.
5. **Számla:** a TEMISZ saját Számlázz.hu-fiókjából, a Számla Agent API-n át, automatikusan. Kártyás fizetésnél a sikeres terhelés után, átutalásnál a díjbekérő kifizetésekor.

## 4. Jogi és adatvédelmi teendők

A webes szolgáltatással a TEMISZ üzemeltetője **adatfeldolgozó** lesz. Az ügyfél az adatkezelő, mert a jogosultak és az eltemettetők személyes adatait tárolja. Ehhez kell:

- ÁSZF és előfizetési feltételek;
- adatfeldolgozói szerződés (GDPR 28. cikk), amit az ügyfél a regisztrációkor fogad el;
- adatkezelési tájékoztató és impresszum;
- csak a működéshez szükséges sütik, így nem kell sütisáv;
- incidenskezelési eljárás (72 órás bejelentési kötelezettség).

**Ezeket ügyvéddel kell átnézetni. A program csak a sablonok helyét és az elfogadás rögzítését adja.**

## 5. Biztonság

- HTTPS mindenhol, HSTS, biztonságos és `SameSite` sütik, CSRF-védelem.
- Belépési kísérletek korlátozása IP-cím és fiók szerint (a zárolás már megvan).
- Kétlépcsős azonosítás (TOTP) a rendszergazdáknak kötelező, másoknak választható.
- A változásnapló (hash-lánc) marad, kiegészül az IP-címmel és a böngészővel.
- Ügyfelenkénti titkosítási kulcs, a szerveren külön tárolva az adatoktól.
- Felügyelet: elérhetőségfigyelés, hibanapló, riasztás e-mailben.
- Az éles indulás előtt külső biztonsági átvizsgálás javasolt.

## 6. Átállás a letölthető programról

A „Beállítások › Mentés” alatt készített mentés és a helyreállító kulcs feltölthető a webes fiókba. A rendszer ellenőrzi a mentés sértetlenségét, és változatlanul átveszi a folyószámokat és a változásnaplót.

## 7. Weboldal

- A „Letöltés” helyét a „Kipróbálom 30 napig ingyen” (regisztráció) és a „Belépés” veszi át.
- Új „Árak” rész: egy csomag, egy ár, mi van benne, hogyan lehet fizetni.
- A telepítési útmutató és a letöltési táblázat lekerül.

## 8. Ütemezés

| Szakasz | Tartalom | Becsült munka |
| --- | --- | --- |
| W1 | Szerver (axum), bérlőnkénti adatbázis, bejelentkezés sütivel, a mostani felület böngészőből | 1–1,5 hét |
| W2 | Regisztráció, e-mail-megerősítés, jelszó-visszaállítás, meghívás a szervezetbe, kétlépcsős azonosítás | 1 hét |
| W3 | Próbaidő, előfizetés, fizetés, számlázás, csak olvasható mód | 1–1,5 hét |
| W4 | Mentés és replikáció, adatexport, régi mentés feltöltése, fiók törlése | 1 hét |
| W5 | Telefonos nézet (térkép, keresés, adatlap a temetőben), weboldal átalakítása | 1 hét |
| W6 | Üzembe helyezés, felügyelet, terheléses és biztonsági teszt, jogi szövegek helye | 1 hét |
| W7–W12 | Fenntartói és üzemeltetői modulok az [FRS-UZEMELTETES.md](FRS-UZEMELTETES.md) szerint: pénzügy és pénztár, temetésszervezés és naptár, vállalkozók, engedélyek és ügyiratok, üzemeltetési feladatok, fenntartói jelentések | 5–7 hét |

A webes alap (W1–W6) nagyjából 6–8 hét, a teljes üzemeltetői kör további 5–7 hét. Az első kipróbálható webes változat W1 után elkészül. A pénzügyi modul (W7) a W3 után előrehozható, ha a piacra lépéshez az kell először.

## 9. Még nyitott kérdések

1. **Fenntartói modulok:** teljes-e az [FRS-UZEMELTETES.md](FRS-UZEMELTETES.md) listája, és mi a sorrend?
2. **Az ügyfelek saját számlázása:** az önkormányzatok többsége a gazdálkodást az Önkormányzati ASP-ben végzi, és onnan számláz. Ott a TEMISZ a díjat előkészíti és exportálja, a számlát az ASP állítja ki. Egyházközségek és cégek a saját Számlázz.hu-fiókjukkal számlázhatnak közvetlenül a TEMISZ-ből. Ez így megfelel?
3. **Jogi szövegek:** ki készíti, illetve nézi át őket?

## 10. Állapot (2026. november)

| Szakasz | Állapot |
| --- | --- |
| W1–W6 | Kész: Postgres, ügyfelenkénti séma, fiókok, kétlépcsős azonosítás, próbaidő és előfizetés (átutalás, kivonat-párosítás), Számlázz.hu, export és visszaállítás, asztali mentés betöltése, telefonos nézet, weboldal, Vercel-telepítés, jogi tervezetek. |
| W7 | Kész: díjtételek, tételek, számla a temető saját Számlázz.hu-fiókjával vagy ASP-előkészítés, sztornó, befizetések, házipénztár hézagmentes bizonylatszámmal és napi zárással, banki CSV párosítás, kintlévőség, bevételi kimutatás, könyvelési és ASP-export, pénztáros szerepkör. |
| W8–W11 | Kész: helyiségek és temetési naptár ütközésvizsgálattal, sírásási munkalap a foglalásból, ismétlődő munkalapok, temetői dolgozó szerepkör (telefonon lezárás fényképpel), vállalkozók engedélyekkel, munkavégzési bejelentés a temetőfenntartási hozzájárulással, ügyiratok hézagmentes iktatószámmal, határidővel, döntéssel, mellékletekkel és előzményekkel, temetőbejárás fényképpel, gondozatlan sírhely eljárása. |
| W12 | Kész: éves beszámoló a fenntartónak (Excel is), kapacitás-előrejelzés, lejárati értesítő és fizetési emlékeztető e-mailben kézbesítési naplóval, nyilvános sírkereső beágyazó kóddal és az elhunytak elrejtésével. |

Később elkészült: a naptári foglalásból indítható temetésrögzítés (FR-SZE-02), a vállalkozónkénti forgalom és összesítő számla (FR-VAL-03), az ügyiratok nyomtatványai a temető fejlécével, a kiadott példány megőrzésével (FR-ENG-01/04).

Az első próbák után hozzáadva: urna hazavitele és szórás máshol sírhely nélkül (folyószámmal), nyilvános térkép a sírkeresőben a nyitvatartással, automatikus lejárati e-mail emlékeztetők (fél évvel előtte, utána havonta, a nem reagálók listájával és heti összesítővel), közös temetési naptár a meghívott temetkezési irodákkal (saját szerepkör, csak a naptárhoz fér hozzá), interaktív bemutató és Súgó oldal.

Biztonsági és előfizetési átvizsgálás (2026. november): a platform-adminisztrátori jog megerősített címhez és kétlépcsős azonosításhoz kötve, a levelekben lévő hivatkozás nem vehet át idegen címet, belépési napló IP-címmel és böngészővel, a kétlépcsős kód fiókonként is korlátozva, saját jelszócserénél a többi munkamenet kilép, a napi mentés titkosított. Előfizetés: kifizetetlen díjbekérő mellett nem indítható új előfizetés (nem nyitható meg így újra a fiók), a próbaidő alatt rendelt előfizetés a próbaidő végén nem zár le, a csak olvasható fiók is letöltheti egy személy adatait, a korábban fizető ügyfél adatait a rendszer nem törli magától.

Még nyitott a tervből: ügyfelenkénti titkosítási kulcs (az adatbázis a Neonnál titkosítva tárol, de ügyfelenként külön kulcs nincs), külső elérhetőségfigyelés és riasztás, külső biztonsági átvizsgálás.

Lakossági online fizetés: a temető a saját Barion-fiókját köti be (Beállítások › Online fizetés, titkosítva tárolt kulccsal); a számlához fizetési link készül, amely a fizetési emlékeztető levélben is szerepel; a befizető bejelentkezés nélkül, kártyával fizet a /fizetes oldalon, a befizetés automatikusan a számlára kerül.

Árukészlet és leltár (Pénzügy › Készlet): termékek és anyagok nettó árral, ÁFA-val és minimum készlettel; beérkezés, eladás pénztárbizonylattal vagy számlázandó tételként, anyagfelhasználás, leltár az eltérések korrekciójával; a készlet nem mehet mínuszba.

**Még nincs meg** (az FRS-UZEMELTETES „S” és „C” tételeiből):

- FR-FEN-03: több üzemeltető összesített fenntartói nézete (egy fiókon belül az Olvasó szerepkör ad csak olvasó hozzáférést).
- FR-UZE-04, FR-FEN-04, FR-UGY-03 („C”, később).

