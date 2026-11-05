# TEMISZ webes szolgáltatás – terv jóváhagyásra

**Döntés (2026. november 5.):** a TEMISZ böngészőből használható előfizetéses szolgáltatás lesz. Minden új ügyfél 30 napot kap ingyen, utána egységes havidíjat fizet. A letölthető program a 0.1.2-es verzióval lezárul. Aki már használja, át tudja hozni az adatait a webes változatba.

## 1. Mi marad és mi változik

A mostani kód nagy része változatlanul átvihető.

| Rész | Webes változatban |
| --- | --- |
| Felület (React, új dizájn) | Marad, kisebb kiegészítésekkel (regisztráció, előfizetés, telefonos nézet). |
| Üzleti logika (`lunda-core`): folyószám, rátemetés-ellenőrzés, megváltás, napló, PDF, import/export | Marad, a szerveren fut. |
| Adatbázis (titkosított SQLite) | Marad. Minden ügyfél külön adatbázisfájlt kap, külön kulccsal. |
| JSON API (`api::dispatch`) | Marad, HTTPS-en érhető el. A fejlesztői szerver (`lunda-devserver`) már most is így működik. |
| Bejelentkezés (Argon2id, zárolás, időkorlát) | Marad, kiegészül e-mail-megerősítéssel, jelszó-visszaállítással és kétlépcsős azonosítással a rendszergazdáknak. |
| Asztali keret (Tauri), frissítő, helyi mentés, mentésre figyelmeztetés | Megszűnik. Helyette szerveroldali mentés és „Minden adat letöltése” gomb. |
| Internet nélküli működés | Megszűnik. Ezt a weboldalon és az ÁSZF-ben is jelezni kell. |

## 2. Felépítés

- **Szerver:** egy Rust alkalmazás (axum), amely kiszolgálja a felületet és az API-t, és ügyfelenként nyitja meg a megfelelő adatbázist.
- **Ügyfél (bérlő):** egy önkormányzat, egyházközség vagy üzemeltető. Egy ügyfélnek több temetője és több felhasználója lehet. A mostani szerepkörök (rendszergazda, ügyintéző, pénzügyes, olvasó) maradnak.
- **Adatelkülönítés:** ügyfelenként külön SQLCipher-adatbázis. Egy hiba sem tudja összekeverni két ügyfél adatait, és egy ügyfél adata egyben menthető, átadható vagy törölhető.
- **Központi adatbázis:** fiókok, előfizetések, számlák, próbaidő. Nyilvántartási adat nincs benne.
- **Tárhely:** EU-n belüli szerver (például Hetzner, Németország vagy Finnország, vagy magyar szolgáltató). Indulásnak egy gép elég, nagyjából 15–40 EUR havonta.
- **Mentés:** folyamatos replikáció titkosítva, másik helyszínű objektumtárba (Litestream), és napi pillanatképek 30 napig. A visszaállítást havonta ki kell próbálni.

## 3. Próbaidő és előfizetés

1. **Regisztráció** a weboldalon: szervezet neve, adószám (nem kötelező), e-mail, jelszó. E-mail-megerősítés után a fiók azonnal használható. Bankkártya nem kell.
2. **30 nap teljes hozzáférés.** A mintatemető egy kattintással betölthető. A 23. és a 28. napon e-mail-emlékeztető megy.
3. **Lejárat után, ha nincs előfizetés:** a fiók csak olvasható lesz, az adatok letölthetők. 60 nap után előzetes értesítés, majd törlés.
4. **Előfizetés:** egységes havidíj szervezetenként, felhasználószám-korlát nélkül. Két fizetési mód:
   - bankkártyás havi vagy éves fizetés (Stripe vagy Barion);
   - éves díj átutalással, díjbekérő alapján. Önkormányzatoknál és plébániáknál ez a szokásos.
5. **Számla:** NAV-kompatibilis e-számla automatikusan (Billingo vagy Számlázz.hu API).

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

Összesen nagyjából 6–8 hét munka. Az első kipróbálható webes változat W1 után elkészül.

## 9. Döntések, amik Öntől kellenek

1. **Havidíj összege,** és legyen-e kedvezményes éves díj.
2. **Fizetési mód:** bankkártya (Stripe vagy Barion), átutalás díjbekérővel, vagy mindkettő.
3. **Számlázó cég adatai** (név, adószám, bankszámla) és a számlázó szolgáltatás (Billingo vagy Számlázz.hu).
4. **Domain,** például temisz.hu, és ki regisztrálja.
5. **Tárhely:** külföldi (EU) vagy magyar szolgáltató.
6. **Jogi szövegek:** ki készíti, illetve nézi át őket.
