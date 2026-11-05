# TEMISZ – fenntartói és üzemeltetői modulok (FRS-kiegészítés)

**Cél:** a temető fenntartója (önkormányzat, egyházközség) és üzemeltetője (önkormányzati cég, vállalkozás) minden napi ügyét a TEMISZ-ben intézze, a pénzügyekkel együtt. A [FRS.md](FRS.md) nyilvántartási követelményei változatlanul érvényesek. Ez a dokumentum azokat egészíti ki.

**Prioritás:**
- **M** = az induláshoz kell;
- **S** = az első évben;
- **C** = később.

**Jogszabályi háttér:**
- az 1999. évi XLIII. törvény (Ttv.) és a 145/1999. Korm. rendelet (Vhr.);
- a temető saját díjrendelete vagy díjszabása;
- a kegyeleti közszolgáltatási szerződés.

A díjak jogcímeit és az ÁFA-kezelést nem a program dönti el: mindkettő temetőnként beállítható.

## 1. Pénzügy és pénztár

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-PEN-01 | Díjtételek a díjrendelet szerint, temetőnként és érvényességi idővel, például: sírhely-megváltás és újraváltás helytípusonként, sírnyitás, ravatalozó- és hűtőhasználat, temetőfenntartási hozzájárulás, síremlék-engedély. Tételenként beállítható ÁFA-kulcs (mentes, 5%, 27%, tárgyi adómentes), mértékegység és számviteli jogcím. | M |
| FR-PEN-02 | Díjtétel automatikus felajánlása az eseményekhez (temetés, megváltás, újraváltás, engedély). Az ügyintéző módosíthatja, a módosítás oka naplózva. | M |
| FR-PEN-03 | Számlázás Számlázz.hu-n át, a temető saját fiókjával (Számla Agent kulcs): számla, előlegszámla, díjbekérő, sztornó, e-számla e-mailben. A számlaszám és a PDF a TEMISZ-ben marad a kapcsolódó ügy mellett. | M |
| FR-PEN-04 | Önkormányzati ASP-t használó fenntartóknak a kiállítandó tételek exportja: vevő, jogcím, összeg, ÁFA, hivatkozás. A visszakapott számlaszám rögzíthető. | M |
| FR-PEN-05 | Befizetések rögzítése: készpénz, bankkártya, átutalás. Részfizetés, túlfizetés, visszatérítés. Minden befizetés ügyhöz és számlához köthető. | M |
| FR-PEN-06 | Házipénztár: bevételi és kiadási pénztárbizonylat, pénztárnapló, napi zárás egyeztetéssel, a zárt napok nem módosíthatók. | S |
| FR-PEN-07 | Banki átutalások párosítása: bankszámlakivonat (CSV) importja, automatikus párosítás közlemény, összeg és név alapján, kézi jóváhagyással. | S |
| FR-PEN-08 | Kintlévőségek: lejárt tartozások ügyfelenként, fizetési felszólítás nyomtatva és e-mailben, a felszólítások naplója. | M |
| FR-PEN-09 | Bevételi kimutatás jogcímenként, időszakra és temetőnként; ÁFA-összesítő; könyvelési export (CSV vagy Excel, beállítható főkönyvi számokkal). | M |
| FR-PEN-10 | Pénzügyes szerepkör: csak a pénzügyi részhez fér hozzá, a nyilvántartást csak olvassa. Pénztáros szerepkör: csak a pénztárhoz. | M |

## 2. Temetésszervezés és naptár

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-SZE-01 | Temetési naptár: időpontfoglalás temetőnként, a ravatalozó, a terem, a hűtő és a sírhely egyidejű foglalásával. Ütközés esetén figyelmeztet. | M |
| FR-SZE-02 | Foglalás a temetkezési szolgáltató kérésére. A temetés rögzítésekor a foglalás adatai átvehetők a nyilvántartó könyvbe. | M |
| FR-SZE-03 | Sírásási és előkészítési munkalap: hely, időpont, mélység, koporsó vagy urna, felelős. Nyomtatható, és telefonon a temetőben kipipálható. | M |
| FR-SZE-04 | Napi és heti nézet nyomtatva a temetőgondnoknak. | S |

## 3. Temetkezési szolgáltatók és vállalkozók

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-VAL-01 | Vállalkozók nyilvántartása: temetkezési szolgáltató, kőfaragó, kertész. Adatok, engedélyek érvényessége, kapcsolattartó. | M |
| FR-VAL-02 | Munkavégzés bejelentése a temetőben (temetés, síremlék-állítás, bontás), a temetőfenntartási hozzájárulás díjának automatikus számításával és számlázásával. | M |
| FR-VAL-03 | Vállalkozónkénti forgalom és tartozás, havi összesítő számla lehetősége. | S |

## 4. Engedélyek, szerződések, ügyiratok

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-ENG-01 | Ügytípusok sablonnal: sírhely-megváltási szerződés, újraváltás, síremlék-állítás, -felújítás és -bontás engedélye, exhumálás, áthelyezés, hamvak kiadása, rendelkezési jog átruházása. | M |
| FR-ENG-02 | Ügyiratszám (iktatás) temetőnként és évente folyamatosan. Az ügy állapota: beérkezett, hiánypótlás, engedélyezve, elutasítva, lezárva. Határidő-figyelés. | M |
| FR-ENG-03 | Dokumentumok csatolása (PDF, fénykép, beszkennelt papír) az ügyhöz, a sírhelyhez és az elhunythoz. Hozzáférés jogosultság szerint. | M |
| FR-ENG-04 | Nyomtatványok a sablonokból, a temető fejlécével, aláírásra. A kiadott dokumentum változatlan másolata megőrzésre kerül. | M |

## 5. Üzemeltetés

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-UZE-01 | Karbantartási feladatok: kaszálás, hulladékszállítás, sétány- és kerítésjavítás, fakivágás. Felelős, határidő, ismétlődés, állapot. | S |
| FR-UZE-02 | Temetőbejárás telefonon: a sírhelyek állapotának rögzítése (elhanyagolt, balesetveszélyes, felszámolandó) fénykép és térképhely megadásával. | S |
| FR-UZE-03 | Elhanyagolt sírhelyek eljárása: felszólítás a jogosultnak, határidő, a felszámolás dokumentálása. | S |
| FR-UZE-04 | Eszközök és létesítmények (ravatalozó, hűtő, gépek) nyilvántartása, karbantartási és felülvizsgálati határidőkkel. | C |

## 6. Fenntartói jelentések és felügyelet

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-FEN-01 | Éves beszámoló a fenntartónak: temetések, megváltások, díjbevétel jogcímenként, kintlévőség, szabad és foglalt helyek, elvégzett feladatok. | M |
| FR-FEN-02 | Kapacitás-előrejelzés: szabad helyek parcellánként, a temetési számok alapján várható betelés, az újraértékesíthető helyek. | S |
| FR-FEN-03 | Fenntartói (csak olvasó) hozzáférés az üzemeltető fiókjához, vagy több üzemeltető adatainak összesítése egy fenntartói nézetben. | S |
| FR-FEN-04 | Díjrendelet-előkészítés: a díjtételek bevétele és a változtatás hatása a következő évre. | C |

## 7. Ügyfélkapcsolat és nyilvánosság

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-UGY-01 | Értesítők e-mailben is (lejárat, felszólítás, díjbekérő), a kézbesítés naplójával. Levél nyomtatása, ha nincs e-mail-cím. | M |
| FR-UGY-02 | Nyilvános sírkereső a temető vagy az önkormányzat honlapjára beágyazva (FR-KES-01). Csak a hozzájárulással közzétehető adatok jelennek meg. | S |
| FR-UGY-03 | Online díjfizetés a hozzátartozóknak a díjbekérőből (kártyás fizetés a temető saját fizetési fiókjába). | C |

## 8. Előfizetéshez kötött működés

| ID | Követelmény | Prior. |
| --- | --- | --- |
| FR-ELO-01 | Temető létrehozása előtt a program megmutatja, mennyivel nő a díj (temetőnként havi 45 000 Ft + ÁFA vagy éves 450 000 Ft + ÁFA). Az archivált temető nem számít bele a díjba, de csak olvasható. | M |
| FR-ELO-02 | Próbaidő: 30 nap, tetszőleges számú temetővel. A próbaidő végén a fiók rendszergazdája választ temetőszámot és fizetési módot. | M |
