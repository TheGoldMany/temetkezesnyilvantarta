# Nagyvölgyi köztemető – tesztadatok

Kitalált, de életszerű temető a TEMISZ kipróbálásához. Minden név és adat kitalált.

- 4212 temetés 1955-től 2026-ig, folyószámmal, időrendben
- 2198 temetési hely tíz parcellában: egyes és kettős sírhelyek, gyermeksírok, díszsírhelyek, urnafülkék és urnasírhelyek
- koporsós temetések, rátemetések a nyugvási idő után, urnák elhelyezése sírba
- rendelkezési jogosultak címmel és telefonszámmal, megváltások (sok közülük lejárt, így a Lejáratok oldalon is lesz munka)

## Betöltés

1. Beállítások › Temetők és parcellák: hozzon létre egy temetőt, például „Nagyvölgyi köztemető” néven. A parcellákat és a helyeket az import magától létrehozza.
2. Import és export › Excel/CSV import: válassza ki az első fájlt (`nagyvolgy-01-…csv`).
3. Az oszlopokat a program magától felismeri. Jelölje be: **folyószámok megtartása** és **megváltások létrehozása**.
4. Futtassa le előbb próbaként, majd élesben.
5. Ismételje meg a többi fájllal **sorrendben** (01, 02, … 09). A sorrend számít: a későbbi fájlok a korábbi temetésekre épülnek (rátemetés, urna a sírba).

Egy fájl 500 soros, hogy egy betöltés a szerver időkorlátján belül maradjon.

Új adatsor: `python3 scripts/gen-test-cemetery.py samples/nagyvolgy 500`
