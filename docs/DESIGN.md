# Lunda Temető – felületi irány

**Tárgy:** egy temetőiroda nyilvántartó programja. **Közönség:** 50–70 éves gondnokok, ügyintézők; nem informatikusok, sokszor szemüvegben, ügyféllel szemben ülve. **Fő feladat:** egy temetést hibátlanul rögzíteni, és egy sírt gyorsan megtalálni.

## Irány: „mészkő és moha”
A temető anyagai – mészkő, gránit, moha, zuzmó – adják a palettát; nem krém és terrakotta, nem sötét fekete. Hideg, enyhén zöldes világos alap, palaszürke betű, mélyzöld (moha) akcentus. Sehol nincs díszítő gradiens vagy árnyék-kártyakészlet: a felületek kőlapként, vékony gránitszegéllyel különülnek el.

| Token | Világos | Sötét | Szerep |
| --- | --- | --- | --- |
| `limestone` | #EDF0EB | #151B1D | háttér |
| `slab` | #FFFFFF | #1E272A | felületek, űrlapok |
| `slate` | #1F2A30 | #E3E8E3 | szöveg (≥ 12:1) |
| `granite` | #5B6A70 | #9AA8A3 | másodlagos szöveg (≥ 5:1) |
| `seam` | #C9D0CB | #34413F | szegélyek, elválasztók |
| `moss` | #2F5D45 | #8FC4A3 | elsődleges művelet, kijelölés (AA) |
| `lichen` | #D6E4C8 | #2C3F33 | kijelölt háttér, fókusz-gyűrű kitöltés |

Helyállapot-színek (térkép, jelvények) mindig felirattal és mintázattal együtt: szabad (fehér, szegély), lefoglalt (kék #6E93BF), foglalt (moha #4F7A60), lejárt (okker #C9962E), értesítve (rozsda #A65A2A, csíkos), újraértékesíthető (lila #7D62A0), felszámolt (szürke #7C8486), nem használható (sötét, sraffozott). Védett sír: aranyszínű kettős keret (#B08D2F).

## Betűk
- **Literata** (változó, optikai méretezés) – címek és a sírhely-azonosító. Könyvbetű, a nyilvántartó könyv hangulata; teljes magyar ékezetkészlet (ő, ű).
- **Atkinson Hyperlegible Next** – minden más. Gyengénlátóknak tervezett, jól megkülönböztethető betűformák (I/l/1, O/0) – ügyfélszám, folyószám, dátum olvasásához ideális.
- Alapméret 17 px, sorköz 1,55; skála 17 / 20 / 24 / 30 / 38. Számoknál táblázatos számjegyek. Nincs csupa nagybetűs címke.

## Elrendezés
```
┌──────────────┬───────────────────────────────────────────────┐
│ Lunda Temető │  [ Keresés: név, évszám vagy A/3/12   Ctrl+K ] │
│              ├───────────────────────────────────────────────┤
│ Kezdőlap     │  Oldalcím (Literata 30)          [Fő művelet] │
│ Temetések    │                                               │
│ Temetési h.  │  tartalom, balra zárt, max. 1200 px           │
│ Térkép       │                                               │
│ Személyek    │                                               │
│ Lejáratok    │                                               │
│ Nyomtatványok│                                               │
│ Riportok     │                                               │
│ Import/Export│                                               │
│ Beállítások  │                                               │
│ ──────────── │                                               │
│ Felhasználó  │                                               │
└──────────────┴───────────────────────────────────────────────┘
```
Bal oldali, mindig látható menü (ikon + felirat, 48 px magas sorok). A gyorskereső minden képernyő tetején. Tartalom balra zárt.

## Egyetlen emlékezetes elem
A **sírhely-azonosító tábla**: az „A / 3 / 12” azonosító mindenhol egy kis, kőlapra vésett táblaként jelenik meg (Literata, táblázatos számok, enyhén bemélyedő belső árnyék, vékony kettős szegély). Ez köti össze a keresési találatot, az adatlapot, a térképet és a nyomtatványokat. Minden más visszafogott.

## Elvek
- Egy képernyő = egy feladat; a fő művelet mindig jobb felül, egyértelmű igével („Temetés rögzítése”, nem „Mentés”).
- Hibaüzenet: mi a baj és mit tegyen (pl. „A nyugvási idő 2040. április 1-jén telik le. Addig koporsós rátemetés nem lehetséges.”).
- Teljes billentyűzetes kezelés, látható 3 px-es fókuszgyűrű, `prefers-reduced-motion` tisztelete, nagyítás 90–150 %.
- Mozgás csak válaszként: párbeszédablak nyitása, mentés visszajelzése.
