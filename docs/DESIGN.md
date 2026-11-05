# TEMISZ – felületi irány

**TEMISZ** = **Tem**etőmenedzsment és **I**nformatikai **Sz**oftver. Rövid, modern, jól csengő név.

**Tárgy:** egy temetőiroda nyilvántartó programja. **Közönség:** 50–70 éves gondnokok és ügyintézők; nem informatikusok, sokszor szemüvegben, ügyféllel szemben ülve. **Fő feladat:** egy temetést hibátlanul rögzíteni, és egy sírt gyorsan megtalálni.

## Irány: „gránit és mécses”
A program napi, órákon át tartó munkaeszköz (szuverén testtartás), nem marketingoldal. Ezért:

- sűrű, főkönyvszerű táblák;
- sötét grafit oldalsáv;
- hideg gránitszürke munkafelület.

Egyetlen meleg szín van, a mécseslángszerű borostyán. Ez csak a kijelölést, a fókuszt, a sürgős teendőt és a márkajelet jelöli. A panelek nem keretes kártyák, hanem fehér lapok, felül vastag tintavonallal.

Kerülendő:

- talpas díszbetű;
- krém háttér;
- egyforma, lekerekített kártyák;
- „·” elválasztós meta-sorok;
- üdvözlő szöveg és dísz-statisztikák.

| Token | Világos | Sötét | Szerep |
| --- | --- | --- | --- |
| `limestone` (vászon) | #ECEFF1 | #11161A | háttér |
| `slab` (lap) | #FFFFFF | #182026 | táblák, űrlapok |
| `slate` (tinta) | #141B21 | #E7EBEE | szöveg, elsődleges gomb, lapok felső vonala |
| `granite` | #56626B | #9BA8B1 | másodlagos szöveg |
| `seam` | #D3D9DD | #2B353D | sorvonalak |
| `rail` | #1A2228 | #0C1013 | oldalsáv, weboldal sávja |
| `accent` (mécses) | #E3A21A | #F0B43C | kijelölés, fókusz, sürgős teendő, márkajel |

Helyállapotok (térkép, jelvények), mindig felirattal együtt:

| Állapot | Szín |
| --- | --- |
| szabad | fehér, kerettel |
| lefoglalt | #4F78B0 |
| foglalt | #6B8574 |
| lejárt | #E3A21A |
| értesítve | #B4502A, csíkozva |
| újraértékesíthető | #7A5CA8 |
| felszámolt | #9AA3A8 |
| nem használható | #2A3137, sraffozva |

## Betű
Egyetlen család: **Atkinson Hyperlegible Next**, gyengénlátóknak tervezett betű. Jól megkülönböztethető alakjai (I/l/1, O/0) segítenek a folyószám és a dátum olvasásában. Címekhez 800-as, sorközi feszességgel. Alapméret 17 px. A PDF-nyomtatványok is ezt használják.

## Az egyetlen emlékezetes elem
A **szegmentált sírhely-címke**: az „A/3/12” azonosító három cellában jelenik meg (parcella | sor | hely), az első cella sötét, mint egy gravírozott címketábla. Ugyanez jelzi a weboldalon a verziót (v | 0 | 1 | 1).

## Elvek
- A kezdőlap munkafelület: teendők (lejárt, értesítésre váró, mentés), legutóbbi bejegyzések táblában, helyállapot-sáv.
- A fő művelet mindig jobb felül; sötét tinta gomb, egyértelmű igével.
- Hibaüzenet: mi a baj és mit tegyen.
- Teljes billentyűzetes kezelés, 3 px borostyán fókuszgyűrű, `prefers-reduced-motion`, nagyítás 90–150 %.
