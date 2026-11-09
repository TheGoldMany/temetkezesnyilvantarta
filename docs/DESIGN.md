# TEMISZ – egységes felület (program és weboldal)

**TEMISZ** = Temetőmenedzsment és Informatikai Szoftver. A logó a „TEMISZ” felirat Jost betűvel, közepes vastagsággal, erősen ritkítva; alatta aranyszínű dupla vonal és a teljes név. Böngészőfül-ikon: a jelkép kicsiben (oszlop, nap, ciprus).

**Tárgy:** egy temetőiroda nyilvántartó programja. **Közönség:** gondnokok és ügyintézők, nem informatikusok. **Fő feladat:** egy temetést hibátlanul rögzíteni, és egy sírt gyorsan megtalálni.

## Irány: Bauhaus, art deco jegyekkel
A program és a weboldal ugyanazt a rendszert használja.

- **Bauhaus az alap:** mértani formák (kör, négyzet, háromszög), szögletes elemek (0–3 px lekerekítés, csak a pontok és jelek kerekek), aszimmetrikus rács, tiszta színfelületek, árnyék helyett kemény eltolt kobalt „blokk” a kiemelt ablakokon.
- **Art deco jegyek, takarékosan:** aranyszínű (gilt) dupla vonal a címek alatt és a szakaszok elején, a jelkép napsugara és lépcsős talapzata, a lábléc dupla aranyvonala.
- **A jelkép** a közös motívum: a temető Bauhaus-formákra bontva (kapuoszlop, lenyugvó nap, ciprusok háromszögként), napsugárral és lépcsős talapzattal. A weboldal nyitóképén és a program belépőképernyőjén jelenik meg. A térképen a ciprusfasor is háromszögekből áll.
- Egyetlen erős elem oldalanként: a weboldalon a nyitókép jelképe, a programban a térkép.

Kerülendő:

- krém háttér és talpas díszbetű, kapszula alakú gombok;
- puha szürke árnyékok és egyforma kártyarács;
- csupa nagybetűs feliratok (a logó kivétel), „·” elválasztós meta-sorok;
- egyetlen kiemelt szó a címben, díszítő számozás (a lépések valódi sorrendet jelölnek, ott a szám körben áll).

## Betű
Jost (a Futura mértani hagyományában, változtatható vastagság), egyetlen család. Címek 500-as vastagsággal, enyhén szűkített betűközzel; szöveg 400, kiemelés 560–600; a logó 600, 0,22–0,24 em ritkítással. Számok táblázatos számjegyekkel.

## Színek

| Token | Világos | Sötét | Szerep |
| --- | --- | --- | --- |
| `limestone` (vakolat) | #EDEDEA | #111318 | háttér |
| `slab` (lap) | #FFFFFF | #1A1D24 | lapok, táblák, űrlapok |
| `slate` (tinta) | #16181D | #E8E8E4 | szöveg, keretek a kiemelt elemeken |
| `granite` | #4A4E57 | #A3A7AF | másodlagos szöveg |
| `seam` | #D6D6D1 | #2E323B | sorvonalak |
| `moss` (kobalt) | #1E3F8C | #8FA8E8 | elsődleges gomb, aktív menüpont, helyazonosító |
| `rail` (mély kobalt) | #142A5E | #0D1530 | belépőkép, lábléc |
| `accent` (okker) | #DDA22C | #EAB441 | lejáró, sürgős, fókusz |
| `alert` (cinóbervörös) | #B8382A | #F0A196 | hiba, végleges művelet; a jelképben díszítő szín |
| `gilt` (arany) | #A8873A | | art deco vonalak |

Helyállapotok (térkép, jelvények), mindig felirattal együtt:

| Állapot | Szín |
| --- | --- |
| szabad | fehér, kerettel |
| lefoglalt | #3F5E9E |
| foglalt | #8B8F97 |
| lejárt | #DDA22C |
| értesítve | #B8562A, csíkozva |
| újraértékesíthető | #6E5A9E |
| felszámolt | #A9ABB0 |
| használhatatlan | #3A3D44, sraffozva |

## Elrendezés
- **Program:** felső sáv (felirat, temető, gyorskeresés Ctrl+K, felhasználó), alatta vízszintes menü a tíz résszel, aktív pont kobalt aláhúzással. A tartalom legfeljebb 1320 px széles.
- **Weboldal:** ugyanez a sáv és ablak jelenik meg a nyitóképen. Teljes szélességű, eltérő tónusú szakaszok, 1320 px-es tartalomszélesség. Telefonon „Menü” gomb nyitja a teljes képernyős menüt.
- **Helyazonosító:** szegmentált tábla (parcella | sor | hely). Az első szegmens kobalt alapon fehér, szögletes keret.
