import { useState } from "react";
import { Compass, Search } from "lucide-react";
import { Button, Input, PageHeader } from "../../components/ui";
import { startTour } from "./Tour";

/** Súgó: step-by-step guides for every task, and the guided tour. */

interface Guide {
  id: string;
  title: string;
  steps?: string[];
  text?: string[];
  tips?: string[];
}

const GUIDES: { group: string; items: Guide[] }[] = [
  {
    group: "Első lépések",
    items: [
      {
        id: "start",
        title: "Hogyan kezdjek hozzá?",
        steps: [
          "Beállítások › Üzemeltető: töltse ki az üzemeltető nevét, címét, telefonszámát és e-mail-címét. Ezek kerülnek a nyomtatványok fejlécére és a levelekre.",
          "Beállítások › Temetők és parcellák: hozza létre a temetőt, majd a parcellákat (lásd lent).",
          "Beállítások › Helytípusok és Díjtételek: ellenőrizze a megváltási időket és a díjrendelet szerinti díjakat.",
          "Ha van régi nyilvántartása, töltse be: Beállítások › Adatok › Import és export (Excel vagy CSV).",
          "Hívja meg a kollégáit: Beállítások › Felhasználók.",
        ],
        tips: ["A bemutatót bármikor újraindíthatja ezen az oldalon, a „Bemutató indítása” gombbal."],
      },
      {
        id: "testdata",
        title: "Kipróbálás tesztadatokkal",
        text: [
          "Az első belépéskor betölthet egy kis mintatemetőt. Nagyobb próbához a TEMISZ-csapattól kérhet egy kitalált, „Nagyvölgyi köztemető” nevű adatsort (több mint 4000 temetés, 2200 sírhely), kilenc CSV-fájlban.",
        ],
        steps: [
          "Hozzon létre egy temetőt, például „Nagyvölgyi köztemető” néven.",
          "Beállítások › Adatok › Import és export › Excel/CSV import: válassza ki az első fájlt.",
          "Jelölje be a „folyószámok megtartása” és a „megváltások létrehozása” lehetőséget.",
          "Futtassa le próbaként, majd élesben. Ismételje meg a többi fájllal, sorrendben.",
        ],
      },
    ],
  },
  {
    group: "Temető, parcellák, térkép",
    items: [
      {
        id: "cemetery",
        title: "Új temető létrehozása",
        steps: [
          "Beállítások › Temetők és parcellák › Új temető.",
          "Adja meg a nevét, címét, helyrajzi számát, a fenntartót és az üzemeltetőt, valamint a nyitvatartást (ez a nyilvános sírkeresőben is látszik).",
          "Folyószámozás: válassza ki, hogy a folyószám évente újrakezdődik vagy folyamatos.",
        ],
        tips: ["Új temető létrehozása előtt a program megmutatja, mennyivel változik az előfizetés díja."],
      },
      {
        id: "parcel",
        title: "Új parcella létrehozása",
        steps: [
          "Beállítások › Temetők és parcellák: válassza ki a temetőt a listában.",
          "Kattintson a „Parcellák és sorok létrehozása” gombra.",
          "Parcella jele: például A, B vagy Urnafal. Megnevezés: például „Régi temetőrész” (nem kötelező).",
          "Sorok: ettől–eddig (például 1–12), helyek száma soronként (például 30), az első hely száma (általában 1).",
          "Válassza ki a helytípust (egyes sír, kettős sír, urnafülke stb.), majd hozza létre.",
          "A program létrehozza az összes helyet A/1/1, A/1/2 … formában, és felrajzolja a parcellát a térképre, a többi mellé.",
        ],
        tips: [
          "Egy meglévő parcellát bővíthet is: adja meg ugyanazt a betűjelet és az új sorokat. A már meglévő helyek nem duplázódnak.",
          "Vegyes parcellánál hozza létre a sorokat több lépésben, más-más helytípussal.",
          "Import közben a hiányzó parcellák és helyek maguktól létrejönnek.",
        ],
      },
      {
        id: "map",
        title: "A térkép szerkesztése",
        steps: [
          "Térkép menü › Szerkesztés gomb (ehhez „Térkép szerkesztése” jog kell).",
          "Húzza a parcellát a valódi helyére: a parcella keretét megfogva az összes sír együtt mozog.",
          "Egy-egy sírt külön is mozgathat. A forgatás gomb 15 fokkal fordítja el a kijelöltet.",
          "Kettős sír: jelöljön ki két szomszédos helyet (Shift + kattintás), majd Egyesítés.",
          "A végén nyomja meg a „Térkép mentése” gombot. Az Elvetés visszaállítja az előző állapotot.",
        ],
        tips: [
          "Navigálás: nyilak a mozgatáshoz, + és − a nagyításhoz, 0 a teljes nézethez. Az „Ugrás helyre” mezőbe írja be például: A/3/12.",
          "A térkép PDF-be is menthető A4-től A0-ig, a temetői hirdetőtáblára.",
        ],
      },
    ],
  },
  {
    group: "Nyilvántartás",
    items: [
      {
        id: "burial",
        title: "Temetés rögzítése",
        steps: [
          "Kezdőlap › Új temetés rögzítése (vagy egy sírhely adatlapján: Rátemetés).",
          "Elhunyt: név, születési adatok, anyja neve, lakóhely, halálozás. Ha már szerepel a nyilvántartásban, a program felajánlja.",
          "Hely: válassza ki a temetés módját és a helyet. A program ellenőrzi a nyugvási időt és a hely szabályait.",
          "Eltemettető és rendelkezési jogosult: ha a helynek még nincs jogosultja, itt kell megadni.",
          "Megváltás: a program javasolja az időtartamot és a díjat a díjrendelet alapján.",
          "Ellenőrzés után a bejegyzés folyószámot kap.",
          "Nyomtatványcsomag: a program előre kijelöli, amire szükség lesz: gyászlap, temetési értesítő a hirdetőtáblára, sírásási munkalap térképrészlettel, ideiglenes sírjel, megváltási igazolás, díjbekérő, felvilágosítás a családnak. A „Kijelöltek nyomtatása” gomb egy PDF-ben adja ki az összeset.",
        ],
        tips: [
          "A csomagot később is kinyomtathatja: a sírhely adatlapján, a temetések között a „Nyomtatványok” gombbal.",
          "A gyászlapra kérésre keresztet tehet. A program megjegyzi a választást.",
          "A temetési értesítőn a nap és az óra a naptárbeli foglalásból jön. Ha nincs foglalás, csak a dátum szerepel.",
        ],
      },
      {
        id: "exhumation",
        title: "Exhumálás, áthelyezés, urnakiemelés",
        steps: [
          "A sírhely adatlapján válassza az Áthelyezés vagy az Urnakiemelés lehetőséget.",
          "Jelölje, hogy a rendelkezési jogosult hozzájárult a sírnyitáshoz (145/1999. Korm. rendelet 35. § (2)). Az írásos hozzájárulást csatolja vagy őrizze meg.",
          "Koporsós temetéstől számított 15 éven belül adja meg a népegészségügyi engedély számát (36. §). Enélkül a program nem rögzíti.",
          "Az exhumálásról legalább 10 nappal előtte hirdetményt kell kifüggeszteni a temetőben (40. §).",
        ],
      },
      {
        id: "offsite",
        title: "Urna hazavitele, szórás máshol",
        text: [
          "Ha az urnát a hozzátartozó hazavitte, vagy a hamvakat nem ebben a temetőben szórták szét, nincs sírhely, de a bejegyzésnek a nyilvántartó könyvben a helye.",
        ],
        steps: [
          "Új temetés rögzítése › Hely lépés › Temetés módja: „Urna hazavitele (nincs sírhely)” vagy „Hamvak szórása máshol”.",
          "Hazavitelnél adja meg az átvevő nevét és címét, szórásnál a helyet.",
          "A bejegyzés folyószámot kap, sírhely, jogosult és megváltás nélkül.",
        ],
        tips: ["Ha az urna egy sírból kerül ki, azt a sírhely adatlapján rögzítse (Urnakiadás)."],
      },
      {
        id: "legal",
        title: "Jogszabályi kötelezettségek, amelyekben a program segít",
        text: [
          "Elektronikus nyilvántartásnál hiteles biztonsági másolat kell (145/1999. Korm. rendelet 41. § (3)): a Nyomtatás és riportok › Nyomtatványok › Nyilvántartó könyv hiteles nyomtatása kötetazonosítóval és ellenőrzőkóddal készül, a Kezdőlap figyelmeztet, ha egy hónapja nem készült. A program naponta automatikus mentést is készít.",
          "A nemzeti sírkert részét képező sírok adatait át kell adni a NÖRI-nek (1999. évi XLIII. törvény 18. § (9)): Beállítások › Adatok › Import és export › Nemzeti sírkert adatai.",
          "A temetőn kívül elhelyezett urnáról nyilatkozat kell, amelyet a temető fennállásáig meg kell őrizni (18. § (1a), (8)): az urna-átvételi elismervény ezt tartalmazza.",
          "A temetőtérképet a bejáratnál ki kell függeszteni és évente frissíteni (17. §): Térkép › Térkép PDF-be.",
        ],
      },
      {
        id: "correct",
        title: "Hibás bejegyzés javítása",
        text: [
          "A nyilvántartó könyvből semmit nem lehet törölni. Elírást a bejegyzés javításával, téves bejegyzést érvénytelenítéssel lehet rendezni, mindkettőt indoklással. A változásnapló minden lépést megőriz.",
        ],
      },
    ],
  },
  {
    group: "Megváltás és lejárat",
    items: [
      {
        id: "expiry",
        title: "Lejáratok kezelése",
        steps: [
          "Lejáratok menü: állítsa be, hány napon belül lejárók látszódjanak.",
          "Jelölje ki a helyeket, majd nyomtasson értesítő levelet, sírcímkét vagy hirdetményt, vagy küldjön e-mailt.",
          "Rögzítse az értesítést (levél, e-mail, címke).",
          "Ha a jogosult nem váltja újra: rögzítsen „Hirdetmény (temető és honlap)” értesítést. Bekapcsolt nyilvános sírkeresőnél a hirdetmény magától megjelenik a honlapon is.",
          "A hely újraértékesíthetővé nyilvánítható, ha a lejárat óta legalább egy év eltelt, és a hirdetmény legalább egy hónapja kint van (145/1999. Korm. rendelet 18. § (5a)). A program ezt ellenőrzi.",
        ],
        tips: ["A legrövidebb használati idő: koporsós sír 25 év (újraváltás 10), sírbolt 60 év (újraváltás 30), urnafülke és urnasír 10 év, urnasírbolt 20 év (újraváltás 5). Rövidebbet a program nem enged."],
      },
      {
        id: "reminders",
        title: "Automatikus e-mail emlékeztetők",
        steps: [
          "Lejáratok menü, lent: kapcsolja be az „Automatikus emlékeztetők” lehetőséget.",
          "A program fél évvel a lejárat előtt, majd a lejárat után havonta e-mailt küld a jogosultnak, amíg újra nem váltja (legfeljebb egy évig).",
          "A „Nem reagáltak” lista mutatja, ki nem jelentkezett több levél után, és akinek nincs e-mail-címe. Nekik egy gombbal nyomtathat levelet.",
          "Az üzemeltető e-mail-címére összesítő megy a kiküldött levelekről és a nem reagálókról.",
        ],
        tips: ["E-mail csak annak megy, akinek a Személyek között meg van adva az e-mail-címe."],
      },
    ],
  },
  {
    group: "Üzemeltetés",
    items: [
      {
        id: "calendar",
        title: "Temetési naptár, közös naptár az irodákkal",
        steps: [
          "Üzemeltetés › Naptár › Helyiségek: vegye fel a ravatalozót, a hűtőt, a termet.",
          "Új időpont: megnevezés, nap, kezdés és befejezés, helyiség, sírhely. Ütközésnél a program szól.",
          "Ha sírhelyet is választ, a program sírásási munkalapot készít a szertartás napjára.",
          "A temetkezési irodát hívja meg: Beállítások › Felhasználók › Meghívás, szerepkör: „Temetkezési iroda (meghívott)”.",
          "A meghívott iroda csak a közös naptárt látja: minden foglalást, így nem foglal ütközően, és saját időpontot vehet fel. A nyilvántartáshoz nem fér hozzá, és csak a saját foglalását módosíthatja.",
          "A temetés megtörténte után a naptárból indítsa a „Temetés rögzítése” gombbal: az adatok előre kitöltődnek.",
        ],
      },
      {
        id: "orders",
        title: "Munkalapok, ügyiratok, bejárás",
        steps: [
          "Munkalapok: sírásás, karbantartás, kaszálás. Lehet ismétlődő (hetente, havonta, évente). A temetői dolgozó telefonon, fényképpel zárja le.",
          "Ügyiratok: iktatás évente folyamatos számmal, határidő, döntés, mellékletek. Az „Irat kiadása” a temető fejlécével készít átvételi elismervényt, hiánypótlást vagy határozatot.",
          "Bejárás: a sírhely állapota (rendben, gondozatlan, balesetveszélyes). Gondozatlan sírnál ügy indul a jogosult felé 60 napos határidővel.",
        ],
      },
    ],
  },
  {
    group: "Pénzügy",
    items: [
      {
        id: "finance",
        title: "Számlázás és pénztár",
        steps: [
          "Beállítások › Díjtételek: a díjrendelet tételei ÁFA-kulccsal és főkönyvi számmal.",
          "Pénzügy › Beállítás: válassza a Számlázz.hu-t (Számla Agent kulccsal) vagy az ASP-előkészítést.",
          "Tételek: a megváltások díja magától bekerül, más tételt kézzel vehet fel. Jelölje ki, majd számlázza.",
          "Befizetés: készpénznél pénztárbizonylat készül. A pénztárnap végén zárja le a napot a megszámolt összeggel.",
          "Bank: töltse be a bankszámlakivonatot (CSV), a program párosítja a befizetéseket a számlákkal.",
        ],
      },
      {
        id: "stock",
        title: "Árukészlet és leltár",
        steps: [
          "Pénzügy › Készlet › Új termék: megnevezés, nettó ár, ÁFA, minimum készlet. Az anyagoknál (homok, cement) kapcsolja ki az „Eladható áru” jelölőt.",
          "Beérkezés: a szállítás után rögzítse a mennyiséget (és ha szeretné, a beszerzési árat).",
          "Eladás: válassza ki a termékeket és a mennyiséget. Készpénznél pénztárbizonylat készül, vagy rögzítheti tételként a számlázáshoz.",
          "Felhasználás: a munkákhoz felhasznált anyagot vonja le a készletből.",
          "Leltár: írja be a megszámolt mennyiségeket (telefonon is). Az eltérést a program korrekcióként rögzíti, a leltár megmarad.",
        ],
        tips: ["Ha egy termék a minimum alá csökken, a Készlet fül tetején figyelmeztetés jelenik meg.", "Nincs elég készlet? A program nem enged eladni; előbb rögzítse a beérkezést."],
      },
    ],
  },
  {
    group: "Nyilvánosság",
    items: [
      {
        id: "public",
        title: "Nyilvános sírkereső a honlapon",
        steps: [
          "Beállítások › Sírkereső: kapcsolja be a közzétételt.",
          "Másolja ki a beágyazó kódot, és tegye a temető vagy az önkormányzat honlapjára.",
          "A látogatók név vagy évszám alapján kereshetnek, és a térképen megnézhetik a sírhelyet. A nyitvatartás is látszik.",
          "Ha egy család kéri, az elhunyt adatlapján kapcsolja ki a megjelenést.",
        ],
        tips: [
          "A kereső csak nevet, évszámokat, temetőt és sírhelyet mutat. Pontos dátumot, lakcímet, jogosultat nem.",
          "A kereső alatt a ma érvényes díjak is látszanak bruttó összeggel, a Beállítások › Díjtételek alapján. Ha nem szeretné, a Sírkereső fülön kapcsolja ki.",
          "A hirdetőtáblára szánt díjtáblázatot a Nyomtatás és riportok oldalon vagy a Díjtételek fülön nyomtathatja ki.",
        ],
      },
      {
        id: "online-pay",
        title: "Online fizetés: a számla kifizetése bankkártyával",
        steps: [
          "Regisztráljon kereskedőként a barion.com-on, és hozzon létre egy elfogadóhelyet a temető nevével.",
          "Beállítások › Online fizetés: adja meg az elfogadóhely titkos kulcsát (Biztonság menü), a Barion-tárca e-mail-címét és a temető fizetési feltételeinek címét.",
          "Pénzügy › Számlák: a számla mellett a „Fizetési link” gomb kimásolja a linket, ezt elküldheti e-mailben vagy SMS-ben.",
          "A fizetési emlékeztető levélben a link magától benne van.",
          "A befizető bejelentkezés nélkül megnyitja, kártyával fizet; a befizetés automatikusan rákerül a számlára, a pénz a temető Barion-tárcájába érkezik.",
        ],
        tips: [
          "Amíg a Barion nem hagyta jóvá az elfogadóhelyet, hagyja kikapcsolva az „Éles fizetés” jelölőt: így a Barion tesztrendszere fut próbakártyákkal.",
          "A fizetési oldal csak a számla számát, összegét, határidejét és a befizető rövidített nevét mutatja, címet és e-mailt nem.",
        ],
      },
    ],
  },
  {
    group: "Fiók és biztonság",
    items: [
      {
        id: "users",
        title: "Felhasználók és szerepkörök",
        text: [
          "Rendszergazda: mindenhez hozzáfér. Ügyintéző: nyilvántartás, üzemeltetés, pénzügy. Pénzügyes: pénzügy és pénztár. Pénztáros: csak a házipénztár. Temetői dolgozó: munkalapok. Olvasó: csak olvas és nyomtat. Temetkezési iroda: csak a közös naptár.",
          "A rendszergazdáknak kötelező a kétlépcsős belépés (hitelesítő alkalmazással).",
          "Ha a szervezetnek több temetője van, a kolléga adatlapján a Temetők résznél kijelölheti, melyikben dolgozhat. A többi temető adatait nem látja, és nem is módosíthatja. A rendszergazdák mindig minden temetőt látnak. A személyek (jogosultak, befizetők) az egész szervezethez tartoznak.",
        ],
      },
      {
        id: "orgs",
        title: "Több szervezet egy belépéssel",
        text: [
          "Aki több önkormányzatnak vagy temetőnek dolgozik, ugyanazzal az e-mail-címmel és jelszóval éri el mindegyiket. A szervezetek közt a felső sávban válthat.",
          "Ha egy szervezet olyan kollégát hív meg, aki máshol már használja a TEMISZ-t, a kolléga nem kap új jelszót: a meglévő belépésével éri el az új szervezetet is.",
          "Új szervezetet (saját előfizetéssel és próbaidővel) a Fiókom › Szervezeteim részen (a jobb felső sarokban a nevére kattintva) hozhat létre. Ehhez nem kell újra regisztrálnia.",
          "Az előfizetés szervezetenként és temetőnként fizetendő. A jelszó és a kétlépcsős belépés minden szervezetben ugyanaz.",
        ],
      },
      {
        id: "backup",
        title: "Mentés, adatletöltés, előfizetés",
        text: [
          "Az adatokról naponta automatikus mentés készül. A Beállítások › Mentés és visszaállítás alatt bármikor letöltheti a teljes nyilvántartást.",
          "Előfizetés temetőnként (Beállítások › Előfizetés): bankkártyával 15 nap próbaidővel, utána automatikus terheléssel, vagy átutalással díjbekérő alapján (a közleménybe írja az ügyfélkódot). A szervezet, a felhasználók és a meghívott irodák ingyenesek.",
          "Előfizetés nélkül, vagy lejárt előfizetéssel a temető csak olvasható; a szervezet többi temetője ettől függetlenül működik. A lemondás az időszak végén lép életbe.",
          "A próbaidő alatt és a mintatemetőben adatot exportálni nem lehet (Excel, teljes letöltés, hiteles könyvek), a nyomtatványokon „PRÓBAIDŐ” jelzés van.",
        ],
      },
    ],
  },
];

export function Help() {
  const [q, setQ] = useState("");
  const needle = q.trim().toLowerCase();
  const match = (g: Guide) =>
    !needle || [g.title, ...(g.steps ?? []), ...(g.text ?? []), ...(g.tips ?? [])].some((x) => x.toLowerCase().includes(needle));
  return (
    <>
      <div data-tour="help">
        <PageHeader
          title="Súgó"
          subtitle="Lépésről lépésre minden feladathoz"
          actions={
            <Button variant="primary" icon={<Compass className="size-5" aria-hidden />} onClick={startTour}>
              Bemutató indítása
            </Button>
          }
        />
      </div>
      <div className="mb-6 max-w-md">
        <label className="relative block">
          <span className="sr-only">Keresés a súgóban</span>
          <Search className="pointer-events-none absolute top-1/2 left-3 size-5 -translate-y-1/2 text-granite" aria-hidden />
          <Input className="pl-10" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Keresés a súgóban, például: parcella" />
        </label>
      </div>
      <div className="flex flex-col gap-8" data-testid="help">
        {GUIDES.map((grp) => {
          const items = grp.items.filter(match);
          if (!items.length) return null;
          return (
            <section key={grp.group}>
              <h2 className="mb-3 text-lg">{grp.group}</h2>
              <div className="flex flex-col gap-2">
                {items.map((g) => (
                  <details key={g.id} id={g.id} open={!!needle} className="group rounded-xl border border-seam bg-slab px-5 py-3 open:pb-5">
                    <summary className="flex min-h-11 cursor-pointer list-none items-center justify-between gap-3 font-semibold">
                      {g.title}
                      <span aria-hidden className="text-granite transition-transform group-open:rotate-90">
                        ›
                      </span>
                    </summary>
                    <div className="mt-2 flex max-w-3xl flex-col gap-3">
                      {g.text?.map((t) => (
                        <p key={t}>{t}</p>
                      ))}
                      {g.steps && (
                        <ol className="flex list-decimal flex-col gap-1.5 pl-6">
                          {g.steps.map((s) => (
                            <li key={s}>{s}</li>
                          ))}
                        </ol>
                      )}
                      {g.tips && (
                        <ul className="flex flex-col gap-1 rounded-lg bg-lichen/40 px-4 py-3 text-[0.95rem]">
                          {g.tips.map((s) => (
                            <li key={s}>Tipp: {s}</li>
                          ))}
                        </ul>
                      )}
                    </div>
                  </details>
                ))}
              </div>
            </section>
          );
        })}
      </div>
    </>
  );
}
