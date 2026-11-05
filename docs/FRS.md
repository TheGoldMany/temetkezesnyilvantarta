# Temetőnyilvántartó szoftver – Funkcionális követelményspecifikáció (FRS)

Nov 5, 2026 · @Richard Uri-Kovacs · Lunda Software · v0.1 tervezet

## 1. Bevezetés

### 1.1 Cél

A dokumentum egy Windowsra és macOS-re letölthető, telepíthető temetőnyilvántartó asztali alkalmazás (munkanév: **Lunda Temető**) funkcionális és nem funkcionális követelményeit rögzíti. A szoftver a magyar temetőüzemeltetők törvényben előírt nyilvántartásait (nyilvántartó könyv, sírbolt könyv) vezeti elektronikusan, a temetési helyek teljes életciklusával (megváltás, újraváltás, lejárat, újraértékesítés) és a temetőtérképpel együtt. A piacon lévő hazai megoldások jellemzően elavultak; a cél egy modern, offline is működő, egyszerűen kezelhető, jogszabálykövető termék.

### 1.2 Hatókör

**Benne van (v1.0):** temetők, parcellák, sorok, temetési helyek kezelése; elhunytak és temetések (koporsós, urnás, rátemetés, urnakiadás, urnakiemelés, hamuszórás, közös sír); rendelkezési jogosultak; megváltás, újraváltás, lejáratfigyelés; díjtételek és befizetések nyilvántartása; síremlékek és védett sírok; keresés és felvilágosítás; temetőtérkép; nyomtatványok és hiteles nyomtatott nyilvántartó könyv; riportok; import (Excel/CSV, régi rendszerek) és export; változásnapló; hiteles biztonsági mentés; licenckezelés és automatikus frissítés.

**Későbbi verzió:** online sírkereső a temető weboldalára, többfelhasználós hálózati szinkron, mobil terepi alkalmazás, számlázóprogram-integráció, drónkép-alapú térképezés.

**Nincs benne:** temetkezési szolgáltatói ügykezelés (koporsóválasztás, szertartásszervezés), könyvelés, NAV-os online számlázás saját motorral.

### 1.3 Célcsoport

| Ügyfélkör | Jellemző igény |
| --- | --- |
| Kis- és közepes települési önkormányzatok (köztemető) | Egy-két temető, 500–10 000 temetési hely, egy-két kezelő, papíralapú vagy Excel-nyilvántartás kiváltása |
| Egyházközségek (felekezeti temető) | Egyszerű kezelés, alacsony ár, offline működés |
| Temetőüzemeltető gazdasági társaságok | Több temető, több felhasználó, riportok, díjbeszedés |

### 1.4 Fogalomtár

| Fogalom | Jelentés |
| --- | --- |
| Temetési hely | Sírhely, sírbolt (kripta), urnasírhely, urnafülke, urnasírbolt, urnakert, hamvasztott maradványok szórására kijelölt terület, közös sír |
| Parcella (sírhelytábla) / sor / hely | A temető számozott, hierarchikus felosztása; a számozást a helyszínen is fel kell tüntetni |
| Megváltás | A temetési hely feletti rendelkezési jog megszerzése díj fejében, meghatározott időtartamra |
| Újraváltás | A rendelkezési jog meghosszabbítása a használati idő lejártakor vagy rátemetéskor |
| Rendelkezési jogosult | Az a személy, aki a temetési hely felett rendelkezik (jellemzően az eltemettető vagy örököse) |
| Eltemettető | Aki a temetésről gondoskodott |
| Nyugvási idő | A koporsós temetéstől számított idő, amely alatt a hely nem bontható meg |
| Rátemetés | További elhunyt elhelyezése meglévő temetési helyre |
| Nyilvántartó könyv | A temetésekről időrendben, folyószámmal vezetett kötelező nyilvántartás |
| Sírbolt könyv | A sírboltokról vezetett kötelező nyilvántartás, a sírboltba temetés rendjével |
| Védett sír | Nemzeti sírkerthez tartozó vagy helyi védelem alatt álló temetési hely, illetve sírjel |

## 2. Jogszabályi háttér és megfelelés

A szoftver a temetőkről és a temetkezésről szóló 1999. évi XLIII. törvény (Ttv.), a végrehajtásáról szóló 145/1999. (X. 1.) Korm. rendelet (Vhr.) és a helyi önkormányzati temetőrendeletek szerint működik. A helyi rendeletek eltérnek (megváltási idők, díjak, értesítési szabályok), ezért ezek **paraméterezhetők**, nem beégetettek. A megfelelést élesítés előtt jogásszal ellenőriztetni kell.

| ID | Követelmény | Forrás | Megvalósítás a szoftverben |
| --- | --- | --- | --- |
| JOG-01 | A sírboltokról sírbolt könyvet, az egyéb temetési helyekről nyilvántartó könyvet kell vezetni | Ttv. 18. § (1) | Két külön nyilvántartás-nézet, közös adatbázisból |
| JOG-02 | A nyilvántartó könyvbe minden temetést (urnaelhelyezést, urnakiadást, urnakiemelést, hamuszórást) időrendben kell bejegyezni | Vhr., helyi rendeletek | Automatikus, folyamatos folyószám; időrendi bejegyzés |
| JOG-03 | Rovatok: folyószám; temetés napja; az elhalt családi és utóneve, születési neve, születési ideje, anyja neve, legutolsó lakóhelye, elhalálozás ideje; sírhelytábla, sor, hely száma; eltemettető, illetve rendelkezési jogosult neve és lakcíme; síremlékre vonatkozó bejegyzések | Vhr. 6. § (2) | Kötelező mezők validációval |
| JOG-04 | A nyilvántartó könyv sorszámozott oldalakból áll, oldalt kivenni, megsemmisíteni tilos; használat előtt hitelesíteni kell | Vhr. 6. § (1) | Bejegyzés nem törölhető, csak indokolt javító bejegyzéssel módosítható; hitelesítési záradékkal nyomtatható, oldalszámozott kimenet |
| JOG-05 | Elektronikus tárolás esetén hiteles biztonsági másolat vagy párhuzamos papíralapú nyilvántartás szükséges | Vhr. (2005-ös módosítás) | Ütemezett, ellenőrzőösszeggel (hash) és időbélyeggel ellátott mentés; időszakos hiteles PDF-kivonat nyomtatásra |
| JOG-06 | A rátemetés feltételeit és a temetési tilalmat (pl. 2 m-en belüli talajvíz) a nyilvántartó könyvbe be kell jegyezni | Vhr. | Temetési helyhez rendelt korlátozás-bejegyzések, figyelmeztetéssel |
| JOG-07 | Közös sírba helyezésnél a közös sír megjelölése és az időpont bejegyzendő | Vhr., helyi rendeletek | „Közös sír” helytípus és esemény |
| JOG-08 | Ismeretlen holttest esetén az eljáró hatóság neve és az ügyszám bejegyzendő | Helyi rendeletek | Feltételes mezők az „ismeretlen” jelölésnél |
| JOG-09 | Más temetőbe szóló befogadó nyilatkozat és temetőn kívüli urnaelhelyezés nyilvántartása: az elhalt azonosító adatai, utolsó lakóhelye, elhalálozás ideje, az elhelyezés helye, a nyilatkozattevő neve és lakcíme | Ttv. 18. § (1a) | Külön nyilvántartási típus |
| JOG-10 | Felvilágosítás a temetési helyről a kegyeleti jogokat gyakorolni kívánóknak; a könyvekbe az eltemettető és a rendelkezési jogosult tekinthet be | Ttv. 18. § (6), Vhr. | Gyorskereső, „felvilágosítás” nyomtatvány csak helymeghatározással, személyes adatok nélkül |
| JOG-11 | Az üzemeltető gondoskodik a személyes adatok biztonságáról | Vhr. 5. § (7) | Titkosított adatbázis, jogosultságkezelés, naplózás (lásd 5. fejezet) |
| JOG-12 | A parcellákat, sorokat és helyeket számozni kell, azonosíthatóan a nyilvántartással | Vhr. | Hierarchikus azonosító (pl. „A/3/12”), térképpel összekötve |
| JOG-13 | Megváltási idők: jellemzően sírhely 25 év, sírbolt 60–100 év, urnasírhely és urnafülke 10–25 év; újraváltás; időarányos visszatérítés lemondáskor | Vhr., helyi rendeletek | Helytípusonként paraméterezhető időtartamok és díjtételek |
| JOG-14 | Nemzeti sírkerthez tartozó hely mentes a megváltási és újraváltási díj alól | Ttv. (2013-as módosítás) | „Védett / nemzeti sírkert” jelölés, díjmentesség automatikusan |
| JOG-15 | Urna kiadásakor az átvételt igazolni és nyilvántartásba venni | Helyi rendeletek | Urnakiadás esemény átvételi elismervény nyomtatvánnyal |
| JOG-16 | A jegyző az ellenőrzés során a nyilvántartásokba betekinthet | Helyi rendeletek | „Ellenőrzési export”: teljes nyilvántartó és sírbolt könyv PDF-ben |

**Adatvédelem (GDPR):** az elhunytak adataira a GDPR nem vonatkozik, a rendelkezési jogosultak, eltemettetők és nyilatkozattevők adataira viszont igen. Ezért a szoftver adatkezelési alapbeállításokat, hozzáférési naplót és érintetti adatexportot biztosít, és az üzemeltetőnek adatkezelési tájékoztató-sablont ad.

## 3. Szereplők és jogosultságok

| Szerepkör | Ki | Mit tehet |
| --- | --- | --- |
| Rendszergazda | Az üzemeltető vezetője vagy a Lunda Software | Felhasználók, szerepkörök, temetők, díjtételek, beállítások, mentés és visszaállítás, licenc |
| Temetőgondnok / ügyintéző | A napi nyilvántartást vezető munkatárs | Temetések, megváltások, újraváltások, befizetések rögzítése; nyomtatványok; keresés; térkép szerkesztése |
| Pénzügyes | Díjbeszedést végző munkatárs | Befizetések rögzítése, díjriportok, nem módosíthat temetési adatot |
| Olvasó | Pl. polgármesteri hivatal, jegyző, ellenőr | Csak keresés, megtekintés, riport és export; semmit nem módosíthat |
| Terepi munkatárs (később) | Sírásó, karbantartó | Munkalapok megtekintése, fotó feltöltése (mobil modul) |

**Jogosultsági szabályok:**

- FR-JOG-01: Minden felhasználó egyéni fiókkal (felhasználónév + jelszó) lép be; közös fiók nem javasolt, de egyfelhasználós módban kikapcsolható a belépés (csak a helyi gépen, figyelmeztetéssel).
- FR-JOG-02: A jogosultságok szerepkör alapúak, a rendszergazda egyéni jogosultságokat is adhat (pl. „befizetés rögzítése”, „bejegyzés javítása”, „export”).
- FR-JOG-03: Bejegyzés javítása csak „javítás” jogosultsággal és kötelező indoklással lehetséges; az eredeti érték a változásnaplóban megmarad.
- FR-JOG-04: Személyes adatok (rendelkezési jogosult lakcíme, telefonszáma) megjelenítése szerepkörhöz köthető; az Olvasó szerepkör alapértelmezésben nem látja.

## 4. Funkcionális követelmények

Prioritás: **M** = kötelező (MVP), **S** = fontos (v1.0), **C** = később.

### 4.1 Temetők és szerkezet

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-TEM-01 | Több temető kezelése egy adatbázisban (név, cím, helyrajzi szám, tulajdonos, üzemeltető, nyitvatartás, jelleg: köz- vagy felekezeti) | M |
| FR-TEM-02 | Hierarchia: temető → parcella (sírhelytábla) → sor → temetési hely, egyedi, emberileg olvasható azonosítóval (pl. „A/3/12”) | M |
| FR-TEM-03 | Parcellák és sorok tömeges létrehozása (pl. „A parcella, 1–20. sor, soronként 30 hely, sírhely típus”) | M |
| FR-TEM-04 | Helytípusok: egyes sírhely, kettős sírhely, gyermeksír, sírbolt (kripta, férőhelyszámmal), urnasírhely, urnafülke, urnasírbolt, urnakert, szóróparcella, közös sír, díszsírhely; a típuslista bővíthető | M |
| FR-TEM-05 | Hely állapotai: szabad, lefoglalt (előre megváltott), foglalt (betemetett), lejárt (újraváltásra vár), értesítés kiküldve, újraértékesíthető, felszámolt, nem használható (pl. talajvíz, út) | M |
| FR-TEM-06 | Helyenkénti jellemzők: méret, férőhely (koporsó / urna), nyugvási idő, korlátozások (temetési tilalom, rátemetés feltételei), megjegyzés | M |

### 4.2 Elhunytak és temetési események (nyilvántartó könyv)

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-ELH-01 | Elhunyt rögzítése: családi és utónév, születési név, nem, születési hely és idő, anyja neve, legutolsó lakóhely, elhalálozás helye és ideje, halottvizsgálati bizonyítvány száma, megjegyzés | M |
| FR-ELH-02 | Eseménytípusok: koporsós temetés, urnaelhelyezés, rátemetés, urnakiadás, urnakiemelés, hamuszórás, áthelyezés (exhumálás és újratemetés), közös sírba helyezés | M |
| FR-ELH-03 | Minden esemény automatikus, egyedi, folyamatos folyószámot kap temetőnként és évenként beállíthatóan; a folyószám nem írható felül, nem hagyható ki | M |
| FR-ELH-04 | Rátemetéskor a rendszer ellenőrzi a nyugvási időt, a férőhelyet és a korlátozásokat, és figyelmeztet, ha a temetés nem engedélyezhető | M |
| FR-ELH-05 | Ismeretlen holttestnél kötelező az eljáró hatóság neve és az ügyszám | M |
| FR-ELH-06 | Más temetőbe szóló befogadó nyilatkozat és temetőn kívüli urnaelhelyezés rögzítése a Ttv. 18. § (1a) szerinti adatokkal | S |
| FR-ELH-07 | Csatolmányok egy eseményhez: beszkennelt bizonyítvány, nyilatkozat, fotó (PDF, JPG, PNG) | S |
| FR-ELH-08 | Ékezethelyes, a magyar névsorrendet követő kezelés (családi név, utónév; dr., özv. előtagok külön mezőben) | M |

### 4.3 Sírbolt könyv

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-SIR-01 | Sírboltonként: sírboltszám, férőhelyek, építés éve, építési engedély vagy terv adatai, rendelkezési jogosult, a jogosult rendelkezései a sírboltba temetés rendjéről | M |
| FR-SIR-02 | Férőhelyenkénti foglaltság megjelenítése (melyik fülkében ki nyugszik) | M |
| FR-SIR-03 | Sírbolt könyv nyomtatható nézete a nyilvántartó könyvvel azonos hitelesítési szabályokkal | M |

### 4.4 Rendelkezési jogosultak és ügyfelek

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-UGY-01 | Személy rögzítése: név, lakcím, levelezési cím, telefon, e-mail, kapcsolat az elhunyttal | M |
| FR-UGY-02 | Egy személy több temetési hely jogosultja is lehet; egy helynek egy fő jogosultja és további kapcsolattartói lehetnek | M |
| FR-UGY-03 | Jogosultváltás (öröklés, átruházás) rögzítése dátummal, jogcímmel, dokumentummal; az előzmény megmarad | M |
| FR-UGY-04 | Duplikált személyek felismerése és összevonása | S |

### 4.5 Megváltás, újraváltás, lejárat

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-MEG-01 | Megváltás: hely, jogosult, kezdő dátum, időtartam (helytípus szerinti alapértelmezés), díj (díjtételből), lejárat automatikus számítása | M |
| FR-MEG-02 | Előre megváltás (lefoglalás) temetés nélkül | M |
| FR-MEG-03 | Újraváltás lejáratkor és rátemetéskor; rátemetésnél a használati idő a helyi szabály szerint a temetéstől számított nyugvási időre hosszabbodik, időarányos díjjal | M |
| FR-MEG-04 | Lemondás és időarányos visszatérítés kiszámítása a megváltáskori díjtételek alapján | S |
| FR-MEG-05 | Lejáratfigyelő: lista a következő 30 / 90 / 365 napban lejáró és a már lejárt helyekről, szűrhetően | M |
| FR-MEG-06 | Értesítés lejáratról: tömeges levélgenerálás (PDF, körlevél), e-mail-sablon, a síron elhelyezhető figyelmeztető címke nyomtatása QR-kóddal, hirdetőtáblára szánt lista | M |
| FR-MEG-07 | Az értesítés tényének és dátumának naplózása; a helyi rendelet szerinti türelmi idő után a hely „újraértékesíthető” állapotba tehető, kötelező megerősítéssel | M |
| FR-MEG-08 | Nemzeti sírkert és védett hely díjmentes kezelése | M |

### 4.6 Díjtételek és befizetések

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-DIJ-01 | Díjtáblázat temetőnként és helytípusonként (megváltás, újraváltás, sírnyitás, ravatalozó-használat, temetőfenntartási hozzájárulás stb.), érvényességi idővel, hogy a régi díjak visszakereshetők maradjanak | M |
| FR-DIJ-02 | Befizetés rögzítése: összeg, mód (készpénz, átutalás, kártya), dátum, bizonylatszám, kapcsolódó esemény | M |
| FR-DIJ-03 | Díjbekérő és nyugta-előkészítő nyomtatvány; számla kiállítása a meglévő számlázóprogrammal (export vagy későbbi API-integráció) | S |
| FR-DIJ-04 | Tartozások és be nem fizetett díjak listája | S |

### 4.7 Síremlékek és védett sírok

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-EML-01 | Síremlék bejelentése és engedélyezése: kivitelező, anyag, méret, engedély dátuma, fotó | M |
| FR-EML-02 | Síremlék-bejegyzések megjelennek a nyilvántartó könyv „síremlékre vonatkozó bejegyzések” rovatában | M |
| FR-EML-03 | Védett sírjel és nemzeti sírkert jelölés, indoklással és a további temetés lehetőségének feltüntetésével | M |
| FR-EML-04 | Állapotfelmérés: elhanyagolt, veszélyes síremlék jelölése fotóval és értesítéssel | C |

### 4.8 Keresés és felvilágosítás

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-KER-01 | Gyorskereső minden képernyőn: elhunyt neve (ékezet- és kisbetű-függetlenül, részleges egyezéssel), születési vagy halálozási év, jogosult neve, hely azonosítója | M |
| FR-KER-02 | Összetett szűrés: temető, parcella, helytípus, állapot, lejárat időszaka, temetés időszaka | M |
| FR-KER-03 | Betűsoros névmutató nyomtatása | M |
| FR-KER-04 | „Felvilágosítás” nyomtatvány a hozzátartozónak: elhunyt neve, temetési hely azonosítója, térképrészlet, személyes adatok nélkül | M |
| FR-KER-05 | Az elhunyt adatlapjáról egy kattintással a térképen a helyre ugrás, és vissza | M |

### 4.9 Temetőtérkép

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-TER-01 | Rácsalapú térképszerkesztő: parcellák téglalapként, sorok és helyek automatikus kiosztással; helyek mozgatása, forgatása, egyesítése (pl. kettős sír) | M |
| FR-TER-02 | Háttérkép betöltése (beszkennelt papírtérkép, légi- vagy drónfotó, PDF) és a rács illesztése rá | S |
| FR-TER-03 | Színkódolt állapotok (szabad, foglalt, lejárt, értesített, védett), jelmagyarázattal; kattintásra a hely adatlapja | M |
| FR-TER-04 | Utak, épületek (ravatalozó, kút, kapu, hulladéktároló) jelölése | S |
| FR-TER-05 | Térkép nyomtatása és PDF-exportja A4–A0 méretben, a bejáratnál kifüggesztendő helyszínrajzhoz | M |
| FR-TER-06 | GPS-koordináta helyenként; később GeoJSON-export térinformatikai rendszerekhez | C |

### 4.10 Nyomtatványok és dokumentumok

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-NYO-01 | Hiteles nyilvántartó könyv és sírbolt könyv nyomtatása: oldalszámozással, folyószám-sorrendben, időszakra szűrve, hitelesítési záradékkal és aláírási hellyel | M |
| FR-NYO-02 | Sablonok: megváltási igazolás, újraváltási igazolás, lejárati értesítő, figyelmeztető címke a sírra, urna-átvételi elismervény, felvilágosítás, díjbekérő, síremlék-engedély | M |
| FR-NYO-03 | Sablonszerkesztő: az üzemeltető fejléce, logója, aláírója, szövegei módosíthatók; mezőhelyettesítés (pl. {elhunyt.nev}) | S |
| FR-NYO-04 | Minden nyomtatvány PDF-ként menthető és a helyhez csatolva archiválódik | S |

### 4.11 Riportok

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-RIP-01 | Kihasználtság temetőnként és parcellánként (szabad, foglalt, lejárt helyek száma) | M |
| FR-RIP-02 | Éves temetési statisztika eseménytípusonként | M |
| FR-RIP-03 | Bevételek időszakra és díjtípusra | S |
| FR-RIP-04 | Lejárati előrejelzés évekre bontva (tervezéshez) | S |
| FR-RIP-05 | Minden riport Excelbe (.xlsx) és PDF-be exportálható | M |

### 4.12 Adatátvétel és export

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-IMP-01 | Import Excelből és CSV-ből, oszlophozzárendelő varázslóval, előnézettel, hibalistával és próbafuttatással | M |
| FR-IMP-02 | Import régi nyilvántartó programokból (Access, dBase, saját CSV) egyedi konverterrel, megrendelésre | S |
| FR-IMP-03 | Papíralapú könyv utólagos rögzítését segítő gyors bevitel: billentyűzetről végig kezelhető űrlap, a régi folyószám megtartásával | M |
| FR-IMP-04 | Teljes export (CSV, Excel, JSON), hogy az üzemeltető ne legyen bezárva a szoftverbe | M |

### 4.13 Változásnapló, mentés, visszaállítás

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-NAP-01 | Változtathatatlan változásnapló: ki, mikor, mit, régi és új érték, indoklás; a napló bejegyzései láncolt ellenőrzőösszeggel védettek | M |
| FR-NAP-02 | Fizikai törlés nincs: a hibás bejegyzés „érvénytelenített” állapotot kap indoklással, és látható marad | M |
| FR-NAP-03 | Automatikus mentés naponta és kilépéskor, beállítható helyre (külső meghajtó, hálózati mappa, felhőmappa), titkosítva; a legutóbbi N mentés megőrzése | M |
| FR-NAP-04 | Hiteles biztonsági másolat: a mentéshez SHA-256 ellenőrzőösszeg, időpont és opcionálisan időbélyeg tartozik; ellenőrző funkció a mentés sértetlenségére | M |
| FR-NAP-05 | Visszaállítás varázslóval, a jelenlegi állapot előzetes mentésével | M |
| FR-NAP-06 | Figyelmeztetés, ha 7 napja nem volt sikeres külső mentés | M |

### 4.14 Beállítások, licenc, frissítés

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-BEA-01 | Üzemeltetői adatok, logó, aláírók, helyi rendelet paraméterei (megváltási idők, nyugvási idők, türelmi idő, díjak) | M |
| FR-BEA-02 | Licenc: kipróbálási időszak (pl. 30 nap, korlátozott helyszámmal), aktiválás licenckulccsal, gépenkénti aktiválás, offline aktiválási lehetőség | M |
| FR-BEA-03 | Automatikus frissítés: értesítés új verzióról, egy kattintásos telepítés, a frissítés előtt automatikus mentés | M |
| FR-BEA-04 | Beépített súgó, első indításkor bemutató varázsló, mintaadatbázis kipróbáláshoz | S |

### 4.15 Későbbi modulok

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-KES-01 | Online sírkereső: a nyilvános adatok (név, évszámok, hely, térképrészlet) közzététele a temető weboldalán, a hozzátartozó hozzájárulásának kezelésével | C |
| FR-KES-02 | Többfelhasználós mód: központi adatbázis (helyi szerver vagy felhő), szinkron több gép között, ütközéskezeléssel | C |
| FR-KES-03 | Mobil terepi alkalmazás: QR-kód beolvasása a sír címkéjéről, fotó, állapotjelentés | C |
| FR-KES-04 | Számlázóprogram-integráció (Számlázz.hu, Billingo API) | C |

## 5. Nem funkcionális követelmények

### 5.1 Platform és telepítés

| ID | Követelmény |
| --- | --- |
| NFR-PLT-01 | Natív, telepíthető asztali alkalmazás: Windows 10 és 11 (x64, ARM64) és macOS 12 vagy újabb (Apple Silicon és Intel) |
| NFR-PLT-02 | Javasolt technológia: Tauri 2 (Rust háttér + webes felület: React, TypeScript, Tailwind), helyi SQLite adatbázis titkosítással (SQLCipher). Indok: kis telepítő (10–20 MB), alacsony memóriahasználat, egy kódbázis két platformra. Alternatíva: Electron |
| NFR-PLT-03 | Telepítők: Windowson MSI vagy NSIS (.exe), macOS-en .dmg; mindkettő kódaláírással (Windows: kódaláíró tanúsítvány; macOS: Apple Developer ID aláírás és notarizáció), hogy ne jelenjen meg „ismeretlen fejlesztő” figyelmeztetés |
| NFR-PLT-04 | Automatikus frissítés aláírt frissítőcsomagokkal (Tauri updater), a frissítési szerver a Lunda Software tárhelyén |
| NFR-PLT-05 | Letöltés a termék weboldaláról, platformfelismeréssel |

### 5.2 Működés és teljesítmény

| ID | Követelmény |
| --- | --- |
| NFR-TEL-01 | Teljesen offline működés: internet csak a licencaktiváláshoz (offline alternatívával), a frissítéshez és az opcionális felhőmentéshez kell |
| NFR-TEL-02 | 100 000 elhunyt és 50 000 temetési hely mellett a keresés 300 ms alatt, a térkép 1 másodperc alatt jelenik meg egy átlagos irodai gépen (8 GB RAM) |
| NFR-TEL-03 | Indulás 3 másodpercen belül |
| NFR-TEL-04 | Áramszünet vagy összeomlás esetén sincs adatvesztés (tranzakciós adatbázis, WAL-mód) |

### 5.3 Biztonság és adatvédelem

| ID | Követelmény |
| --- | --- |
| NFR-BIZ-01 | Az adatbázis és a mentések titkosítva (AES-256), a kulcs az operációs rendszer kulcstárában (Windows Credential Manager, macOS Keychain) |
| NFR-BIZ-02 | Jelszavak tárolása Argon2 hash-sel; zárolás 5 sikertelen próbálkozás után; automatikus kijelentkezés tétlenség esetén (beállítható) |
| NFR-BIZ-03 | Hozzáférési napló a személyes adatok megtekintéséről és exportjáról |
| NFR-BIZ-04 | Érintetti kérelmek támogatása: egy személy összes adatának exportja; törlési kérelemnél a jogszabály szerint megőrzendő adatok megtartása, a többi anonimizálása |
| NFR-BIZ-05 | Adatfeldolgozói szerződés-sablon az üzemeltető és a Lunda Software között (támogatás és felhőmentés esetére) |

### 5.4 Használhatóság és akadálymentesség

| ID | Követelmény |
| --- | --- |
| NFR-HAS-01 | Teljesen magyar nyelvű felület, magyar dátum- és pénzformátum, magyar ábécé szerinti rendezés (pl. cs, dz, gy, ly, ny, sz, ty, zs) |
| NFR-HAS-02 | Idősebb, nem informatikus felhasználóknak tervezve: nagy, jól olvasható betűk, egyértelmű feliratok, legfeljebb 3 kattintás a leggyakoribb műveletekhez |
| NFR-HAS-03 | Teljes billentyűzetes kezelhetőség a gyors adatbevitelhez; WCAG 2.1 AA kontraszt |
| NFR-HAS-04 | Világos és sötét téma, nagyítható felület |
| NFR-HAS-05 | Kegyeletteljes, visszafogott vizuális megjelenés |

### 5.5 Karbantarthatóság

| ID | Követelmény |
| --- | --- |
| NFR-KAR-01 | Verziózott adatbázis-migrációk, automatikus frissítés az új verzió első indításakor, előzetes mentéssel |
| NFR-KAR-02 | Automatikus tesztek: üzleti szabályok (lejárat, nyugvási idő, folyószám), adatbázis, felület kritikus útvonalai |
| NFR-KAR-03 | Hibajelentés a felhasználó hozzájárulásával, személyes adatok nélkül |

## 6. Adatmodell

Minden entitás kap egyedi azonosítót (UUID), létrehozási és módosítási időbélyeget, létrehozót és módosítót. Törlés helyett érvénytelenítés történik (FR-NAP-02).

| Entitás | Fő mezők | Kapcsolatok |
| --- | --- | --- |
| Temető (cemetery) | név, cím, hrsz., jelleg, tulajdonos, üzemeltető | 1:N Parcella |
| Parcella (section) | jel (pl. „A”), név, alakzat a térképen | N:1 Temető; 1:N Sor |
| Sor (row) | szám, irány | N:1 Parcella; 1:N Temetési hely |
| Temetési hely (plot) | azonosító („A/3/12”), típus, férőhely (koporsó/urna), méret, állapot, nyugvási idő, korlátozások, védett jelölés, térképi geometria, GPS | N:1 Sor; 1:N Megváltás; 1:N Esemény; 1:N Síremlék |
| Sírbolt-férőhely (crypt\_slot) | sorszám, foglaltság | N:1 Temetési hely (sírbolt típus) |
| Helytípus (plot\_type) | név, alapértelmezett megváltási idő, nyugvási idő, férőhely | 1:N Temetési hely; 1:N Díjtétel |
| Elhunyt (deceased) | családi és utónév, előtag, születési név, nem, születési hely és idő, anyja neve, utolsó lakóhely, halálozás helye és ideje, halottvizsgálati bizonyítvány száma, ismeretlen jelölés, hatóság, ügyszám | 1:N Esemény |
| Esemény (interment\_event) | folyószám, típus, dátum, elhunyt, hely, férőhely, eltemettető, megjegyzés, érvényesség | N:1 Elhunyt; N:1 Temetési hely; N:1 Személy |
| Személy (person) | név, lakcím, levelezési cím, telefon, e-mail | N:M Temetési hely (jogosultság); 1:N Befizetés |
| Jogosultság (plot\_holder) | szerep (fő jogosult, kapcsolattartó), kezdet, vég, jogcím | N:1 Személy; N:1 Temetési hely |
| Megváltás (lease) | típus (megváltás, előre megváltás, újraváltás), kezdet, időtartam, lejárat, díj, állapot | N:1 Temetési hely; N:1 Személy; 1:N Értesítés |
| Díjtétel (fee\_rate) | díjtípus, helytípus, összeg, érvényesség kezdete és vége | N:1 Temető |
| Befizetés (payment) | összeg, mód, dátum, bizonylatszám | N:1 Személy; N:1 Megváltás vagy Esemény |
| Értesítés (notice) | típus (levél, e-mail, címke, hirdetmény), dátum, eredmény | N:1 Megváltás |
| Síremlék (monument) | kivitelező, anyag, méret, engedély dátuma, állapot, védett | N:1 Temetési hely |
| Nyilatkozat (external\_placement) | befogadó nyilatkozat vagy temetőn kívüli urnaelhelyezés adatai (Ttv. 18. § (1a)) | N:1 Elhunyt |
| Csatolmány (attachment) | fájl, típus, ellenőrzőösszeg | N:1 bármely entitás |
| Felhasználó (user) | név, szerepkör, jelszó-hash, állapot | 1:N Napló |
| Napló (audit\_log) | időpont, felhasználó, entitás, művelet, régi és új érték, indoklás, előző bejegyzés hash-e | N:1 Felhasználó |
| Beállítás (setting) | kulcs, érték, temetőre vagy globálisan | — |

**Számított értékek:** lejárat = kezdet + időtartam; „temethető-e” = férőhely és nyugvási idő és korlátozások alapján; hely állapota = megváltások, események és értesítések alapján, de kézzel is felülírható indoklással.

## 7. Fő felhasználói folyamatok

### UC-01 Új temetés rögzítése új helyre

1. Az ügyintéző az „Új temetés” gombra kattint.
2. Rögzíti az elhunyt adatait (FR-ELH-01); a rendszer jelzi, ha hasonló nevű elhunyt már szerepel.
3. Kiválasztja a helyet a térképen vagy a listából; csak szabad vagy lefoglalt hely választható.
4. Megadja az eltemettetőt és a rendelkezési jogosultat (új vagy meglévő személy).
5. A rendszer felajánlja a megváltást a helytípus alapértelmezett idejével és díjával.
6. Mentéskor a rendszer kiosztja a folyószámot, a hely állapota „foglalt” lesz, a naplóba bekerül a művelet.
7. Nyomtatható: megváltási igazolás, díjbekérő.

### UC-02 Rátemetés

1. Az ügyintéző a meglévő helyet nyitja meg, és a „Rátemetés” lehetőséget választja.
2. A rendszer ellenőrzi a férőhelyet, a nyugvási időt és a korlátozásokat; ha a temetés nem engedélyezhető, megállít és megindokolja.
3. A helyi szabály szerint kiszámítja az újraváltást (a használati idő meghosszabbítását és az időarányos díjat).
4. Mentés, folyószám, napló, nyomtatványok.

### UC-03 Lejáratkezelés (évente vagy negyedévente)

1. A „Lejáratok” nézetben a gondnok kiszűri a következő évben lejáró és a már lejárt helyeket.
2. Kijelöli őket, és tömegesen elkészíti az értesítő leveleket, a címkéket a sírokra és a hirdetőtábla-listát.
3. A rendszer naplózza az értesítést és dátumát; a helyek állapota „értesítés kiküldve”.
4. Ha a jogosult újravált: befizetés, új lejárat. Ha a türelmi idő eltelik: a gondnok megerősítéssel „újraértékesíthető” állapotba teszi a helyet.

### UC-04 Felvilágosítás hozzátartozónak

1. A hozzátartozó egy elhunyt nevét mondja; az ügyintéző a gyorskeresőbe írja (részleges, ékezet nélküli egyezés is működik).
2. A találatból kinyomtatja a „Felvilágosítás” lapot a hely azonosítójával és a térképrészlettel, személyes adatok nélkül.

### UC-05 Átállás papírról vagy régi programból

1. Az üzemeltető Excelbe gyűjtött vagy régi programból exportált adatait az import varázslóval tölti be, oszlophozzárendeléssel és próbafuttatással.
2. Papírkönyvből a gyors bevitel űrlapon rögzít, a régi folyószámok megtartásával.
3. A térképszerkesztőben elkészíti a parcellákat, és a helyeket hozzárendeli.
4. Az átállás végén a rendszer ellenőrző riportot ad: helyek elhunyt nélkül, elhunytak hely nélkül, lejárat nélküli megváltások.

### UC-06 Hiteles mentés és jegyzői ellenőrzés

1. A rendszer naponta automatikusan ment a beállított külső helyre, ellenőrzőösszeggel.
2. Év végén a gondnok kinyomtatja a hitelesítési záradékkal ellátott nyilvántartó és sírbolt könyvet az adott évre, és aláírja.
3. Jegyzői ellenőrzéskor az „Ellenőrzési export” egy PDF-ben adja a teljes könyveket.

## 8. Ütemezés, nyitott kérdések, források

### 8.1 Verziók

| Verzió | Tartalom | Cél |
| --- | --- | --- |
| MVP (0.1) | Temetők és szerkezet, helyek, elhunytak és események folyószámmal, rendelkezési jogosultak, megváltás és lejárat, keresés, alap rácstérkép, hiteles nyilvántartó és sírbolt könyv nyomtatása, Excel-import, változásnapló, automatikus mentés, Windows- és macOS-telepítő | Egy pilot temetővel élesben kipróbálni |
| v1.0 | Díjtételek és befizetések, értesítések és címkék, sablonszerkesztő, riportok, háttérkép a térképhez, licenc és automatikus frissítés, kódaláírás, súgó | Értékesíthető termék |
| v1.x | Online sírkereső, többfelhasználós szinkron, mobil terepi alkalmazás, számlázó-integráció, GPS és GeoJSON | Bővítések előfizetéses modellben |

### 8.2 Nyitott kérdések

- Melyik a jelenleg használt, elavult hazai szoftver, és milyen formátumban lehet belőle exportálni? Ettől függ az importkonverter.
- Kell-e kezelni a NAV-os számlázást a programon belül, vagy elég a meglévő számlázóprogrammal való integráció?
- Elég-e a hiteles biztonsági másolathoz a hash és a dátum, vagy minősített időbélyeg is szükséges? (Jogi egyeztetés.)
- Üzleti modell: egyszeri licenc és éves támogatási díj, vagy havi előfizetés? Ár temetőnként vagy helyszám szerint?
- Ki lesz a pilot ügyfél? (Kézenfekvő: a lajosmizsei vagy egy környékbeli köztemető, illetve egyházközség.)
- Kell-e angol vagy német felület (pl. határon túli magyar egyházközségeknek)?

### 8.3 Források

- 1999. évi XLIII. törvény a temetőkről és a temetkezésről, 18. § (2013. évi CXXXVIII. törvénnyel módosítva)
- 145/1999. (X. 1.) Korm. rendelet, beleértve az elektronikus nyilvántartásra vonatkozó módosítást (hiteles biztonsági másolat vagy párhuzamos manuális nyilvántartás)
- Önkormányzati temetőrendeletek mintái a Nemzeti Jogszabálytárból (megváltási idők, nyilvántartó könyv rovatai, értesítés módja)
- Nemzetközi temetőkezelő szoftverek funkciói (Capterra-összehasonlítások: térképezés színkódolt helyállapotokkal, nyilvános sírkereső, munkalapok, dokumentumtárolás)

A dokumentum tervezet; a jogi követelményeket élesítés előtt jogásszal, a helyi szabályokat a pilot üzemeltetővel kell egyeztetni.
