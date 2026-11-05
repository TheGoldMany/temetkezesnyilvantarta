# Nyitott kérdések és választott alapértelmezések

Az FRS-ben talált ellentmondások és hiányok. Mindegyiknél a választott, biztonságos alapértelmezés szerepel; ezek a Beállításokban paraméterezhetők, ahol lehet.

| # | Kérdés | FRS-hivatkozás | Választott alapértelmezés |
| --- | --- | --- | --- |
| Q1 | A folyószám temetőnként folyamatos, vagy évente újraindul? | FR-ELH-03 („temetőnként és évenként beállíthatóan”) | Temetőnként folyamatos, soha nem indul újra; az éves újraindítás beállítható, de csak üres temetőnél váltható. |
| Q2 | A sírboltba temetés a nyilvántartó könyv folyószámát kapja, vagy a sírbolt könyvnek saját sorszáma van? | JOG-01, FR-SIR | Minden esemény egy közös, időrendi folyószám-sorozatot kap; a sírbolt könyv a sírboltokat és a bennük történt eseményeket ugyanezzel a folyószámmal listázza. |
| Q3 | Mennyi az alapértelmezett nyugvási idő (koporsós / urnás)? | FR-TEM-06, JOG-13 | Koporsós: 25 év, urnás: 0 (nincs). Helytípusonként és a helyi rendelet szerint módosítható. A konzervatív érték inkább tilt, mint enged. |
| Q4 | Rátemetéskor pontosan hogyan hosszabbodik a használati idő és hogyan számítandó a díj? | FR-MEG-03 („helyi szabály szerint”) | Új lejárat = max(jelenlegi lejárat, temetés napja + nyugvási idő); díj = meghosszabbított napok / teljes megváltási idő napjai × aktuális megváltási díjtétel, egész forintra kerekítve. |
| Q5 | Az MVP-ben a megváltási díj honnan jön, ha a díjtételek (FR-DIJ) csak v1.0-ban vannak? | 8.1 vs. FR-MEG-01 | Egyszerű díjtétel-tábla (temető × helytípus × díjtípus, érvényességi idővel) már az MVP-ben; befizetések rögzítése nincs. |
| Q6 | A síremlék-bejegyzések (FR-EML) nincsenek az MVP-ben, de a nyilvántartó könyv kötelező rovata (JOG-03). | JOG-03, FR-EML-02 | Helyenkénti egyszerű síremlék-bejegyzés (dátum + szöveg) az MVP-ben, a könyv rovatában megjelenik. |
| Q7 | A mentés titkosítási kulcsa: ha a kulcs csak a gép kulcstárában van, gépcsere vagy újratelepítés után a mentés nem állítható vissza. | NFR-BIZ-01, FR-NAP-03/05 | Az első indításkor a rendszer **helyreállító kulcsot** generál, amelyet ki kell nyomtatni és el kell tenni; a mentések ezzel titkosítódnak. |
| Q8 | Elég-e a hiteles másolathoz a SHA-256 + időpont, vagy minősített időbélyeg kell? | JOG-05, 8.2 | SHA-256 + helyi időpont; az időbélyeg-szolgáltató bekötése előkészítve, de kikapcsolva. |
| Q9 | „Oldalt kivenni tilos”: a hiteles nyomtatás oldalszámozása kötetenként folytatólagos legyen-e (több nyomtatás között)? | JOG-04, FR-NYO-01 | Minden hiteles nyomtatás külön, naplózott „kötet” azonosítóval, saját 1-től induló oldalszámozással és tartalmi kivonattal (hash) a láblécben. |
| Q10 | Egyfelhasználós módban kikapcsolható a belépés (FR-JOG-01) – ez ütközik a JOG-11 adatbiztonsági követelménnyel. | FR-JOG-01, JOG-11 | Alapértelmezésben a belépés kötelező; kikapcsolni csak a rendszergazda tudja, figyelmeztetéssel, és a napló „helyi felhasználó” néven rögzít. |
| Q11 | Az MVP-ben kell-e az értesítés (FR-MEG-06/07), ha a megbízás csak FR-MEG-01–05-öt sorol fel, de az UC-03 E2E-teszt kéri? | FR-MEG-06/07, UC-03 | Az értesítés tényének rögzítése, a lejárati értesítő levél és a sírcímke (FR-NYO-02 része) elkészül; e-mail-küldés és körlevél-motor nem. |
| Q12 | Zárolás feloldása 5 sikertelen belépés után: ki és mikor? | NFR-BIZ-02 | 15 perc után automatikusan, vagy rendszergazda azonnal feloldja. Az egyetlen rendszergazdához helyreállító kulcs tartozik. |
| Q13 | Mely helytípusokhoz tartozik koporsós férőhely, melyekhez urna? | FR-TEM-04, FR-TEM-06 | Helytípusonként alapértelmezett koporsó- és urnaférőhely (pl. egyes sírhely: 1 koporsó + 4 urna; kettős: 2 + 8; urnafülke: 0 + 2; sírbolt: férőhelyszám szerint), helyenként felülírható. |
