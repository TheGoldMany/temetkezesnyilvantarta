# TEMISZ – egységes felület (program és weboldal)

**TEMISZ** = Temetőmenedzsment és Informatikai Szoftver. A logó maga a „TEMISZ” felirat: Instrument Sans, félkövér, keskenyített vágás, ritkított betűközzel. Külön jel nincs.

**Tárgy:** egy temetőiroda nyilvántartó programja. **Közönség:** gondnokok és ügyintézők, nem informatikusok. **Fő feladat:** egy temetést hibátlanul rögzíteni, és egy sírt gyorsan megtalálni.

## Irány: „ciprus és körömvirág”
A program és a weboldal ugyanazt a rendszert használja:

- hűvös, zöldesszürke vászon;
- fehér lapok 12 px-es lekerekítéssel, halvány kerettel;
- egyetlen márkaszín, a ciprus zöld;
- a körömvirág-sárga csak a lejárót, a sürgőset és a billentyűzetfókuszt jelöli.

A temetőtérkép parcelláinak motívuma a közös jelkép. Megjelenik a weboldal nyitóablakában, a program belépőképernyőjén és magában a térképnézetben is.

Kerülendő:

- krém háttér és talpas díszbetű;
- egyforma kártyarács, kapszula alakú gombok;
- csupa nagybetűs feliratok, „·” elválasztós meta-sorok;
- egyetlen kiemelt szó a címben, díszítő számozás.

## Betű
Instrument Sans (változtatható vastagság és szélesség), egyetlen család. Címek 650-es vastagsággal, 80–86% szélességgel. Szöveg 400, kiemelés 600–640. Számok táblázatos számjegyekkel.

## Színek

| Token | Világos | Sötét | Szerep |
| --- | --- | --- | --- |
| `limestone` (vászon) | #ECF0EC | #0F1A16 | háttér |
| `slab` (lap) | #FFFFFF | #16241F | lapok, táblák, űrlapok |
| `slate` (tinta) | #17251F | #E6EEE9 | szöveg |
| `granite` | #4C5A53 | #9FB2A8 | másodlagos szöveg |
| `seam` | #D9DFDA | #2A3B34 | keretek, sorvonalak |
| `moss` (ciprus) | #1F5546 | #7FC2A6 | elsődleges gomb, aktív menüpont, helyazonosító |
| `rail` (mély ciprus) | #143B31 | #0B1411 | belépőkép, értesítések, weboldal sötét sávjai |
| `accent` (körömvirág) | #E2A32B | #F0B43C | lejáró, sürgős, fókusz |

Helyállapotok (térkép, jelvények), mindig felirattal együtt:

| Állapot | Szín |
| --- | --- |
| szabad | fehér, kerettel |
| lefoglalt | #4F6B8A |
| foglalt | #7F9A8D |
| lejárt | #E2A32B |
| értesítve | #B5651D, csíkozva |
| újraértékesíthető | #7D62A0 |
| felszámolt | #9AA39E |
| használhatatlan | #3D4447, sraffozva |

## Elrendezés
- **Program:** felső sáv (felirat, temető, gyorskeresés Ctrl+K, felhasználó), alatta vízszintes menü a tíz résszel, aktív pont ciprus aláhúzással. A tartalom legfeljebb 1320 px széles.
- **Weboldal:** ugyanez a sáv és ablak jelenik meg a nyitóképen. Teljes szélességű, eltérő tónusú szakaszok, 1320 px-es tartalomszélesség. Telefonon „Menü” gomb nyitja a teljes képernyős menüt.
- **Helyazonosító:** szegmentált tábla (parcella | sor | hely). Az első szegmens ciprus alapon fehér.
