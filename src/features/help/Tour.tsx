import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useApp, type Route } from "../../app/context";
import type { Permission } from "../../lib/types";
import { Button } from "../../components/ui";

/**
 * Guided tour for first users: goes through the modules one by one, opens
 * each page and puts a spotlight on the control to use. Starts by itself
 * after the first-run wizard; the Súgó page restarts it.
 */

export const TOUR_KEY = "temisz.tour";
export const TOUR_EVENT = "temisz:tour";

export function startTour() {
  window.dispatchEvent(new Event(TOUR_EVENT));
}

interface Step {
  route?: Route;
  /** data-tour attribute of the element to highlight */
  target?: string;
  title: string;
  body: string;
  /** shown only with this permission */
  needs?: Permission;
}

const STEPS: Step[] = [
  {
    title: "Üdvözöljük a TEMISZ-ben!",
    body: "Ez a bemutató végigvezeti a program fő részein: mire való, hol találja, mivel kezdje. Néhány perc az egész, és bármikor újraindíthatja a Súgó oldalon.",
  },
  {
    route: { view: "dashboard" },
    target: "nav",
    title: "A főmenü",
    body: "Innen éri el a program minden részét. Telefonon oldalra görgethető. Mindig az látszik, amihez a szerepköre jogot ad.",
  },
  {
    route: { view: "settings", tab: "cemeteries" },
    target: "bulk-create",
    title: "1. lépés: temető és parcellák",
    body: "Először hozza létre a temetőt (fent: Új temető), majd itt a parcellákat. Adja meg a parcella betűjelét (például A), hány sor legyen és soronként hány hely, valamint a helytípust. A program egyszerre létrehozza az összes helyet A/1/1, A/1/2 … formában, és felrajzolja őket a térképre.",
    needs: "manage_structure",
  },
  {
    route: { view: "map" },
    target: "map-edit",
    title: "A temető térképe",
    body: "A térkép a parcellákat és a sírhelyeket mutatja, színnel jelölve az állapotukat. A Szerkesztés gombbal húzással a valódi helyükre tolhatja a parcellákat és a sírokat, forgathatja őket, két szomszédos helyet kettős sírrá egyesíthet. A végén nyomja meg a Térkép mentése gombot.",
  },
  {
    route: { view: "plots" },
    target: "nav-plots",
    title: "Temetési helyek",
    body: "Az összes sírhely listája szűrőkkel: parcella, állapot, helytípus, lejárat. Egy helyre kattintva megnyílik az adatlapja: kik nyugszanak ott, ki a rendelkezési jogosult, meddig van megváltva.",
  },
  {
    route: { view: "dashboard" },
    target: "new-burial",
    title: "Temetés rögzítése",
    body: "Ez a leggyakoribb feladat. A varázsló lépésenként kérdez: az elhunyt adatai, a hely (vagy urna hazavitele, szórás máshol), az eltemettető és a jogosult, a megváltás. A program ellenőrzi a nyugvási időt és a hely szabályait, és folyószámot ad.",
    needs: "record_events",
  },
  {
    route: { view: "burials" },
    target: "nav-burials",
    title: "Nyilvántartó könyv",
    body: "Itt látja az összes bejegyzést folyószám szerint. Hibás bejegyzést nem törölni kell, hanem javítani vagy érvényteleníteni, indoklással; minden változás a változásnaplóba kerül.",
  },
  {
    route: { view: "persons" },
    target: "nav-persons",
    title: "Személyek",
    body: "Rendelkezési jogosultak, eltemettetők, kapcsolattartók. Ha megadja az e-mail-címüket, a lejárati értesítők e-mailben is mehetnek.",
  },
  {
    route: { view: "expiries" },
    target: "nav-expiries",
    title: "Lejáratok és emlékeztetők",
    body: "A lejáró és lejárt megváltások. Innen nyomtathat értesítő levelet, címkét a sírra, hirdetményt, vagy küldhet e-mailt. Lent bekapcsolhatja az automatikus emlékeztetőt: fél évvel a lejárat előtt, majd havonta ír a jogosultnak, és jelzi, ki nem reagált.",
    needs: "record_leases",
  },
  {
    route: { view: "operations" },
    target: "new-booking",
    title: "Üzemeltetés: temetési naptár",
    body: "Ide kerülnek a temetési időpontok, a ravatalozó és a hűtő foglalásával; az ütközést a program jelzi. A temetkezési irodákat meghívhatja (Beállítások › Felhasználók, szerepkör: Temetkezési iroda): ők a közös naptárt látják, és maguk foglalhatnak időpontot. Ugyanitt vannak a munkalapok, az ügyiratok, a vállalkozók és a temetőbejárás.",
    needs: "operations",
  },
  {
    route: { view: "finance" },
    target: "nav-finance",
    title: "Pénzügy",
    body: "Díjtételek a díjrendelet szerint, számla a saját Számlázz.hu-fiókjával (vagy ASP-be előkészítve), befizetések, házipénztár napi zárással, banki kivonat párosítása, kintlévőségek.",
    needs: "record_payments",
  },
  {
    route: { view: "prints" },
    target: "nav-prints",
    title: "Nyomtatványok",
    body: "A hiteles nyilvántartó könyv és sírbolt könyv, mutató, igazolások, megváltási okirat, díjbekérő, térkép PDF-ben.",
  },
  {
    route: { view: "reports" },
    target: "nav-reports",
    title: "Riportok és éves beszámoló",
    body: "Kihasználtság parcellánként, éves statisztika, és az éves beszámoló a fenntartónak a kapacitás-előrejelzéssel együtt. Mind letölthető Excelben is.",
  },
  {
    route: { view: "importExport" },
    target: "nav-importExport",
    title: "Adatok betöltése",
    body: "A régi nyilvántartást Excelből vagy CSV-ből töltheti be: a program felismeri az oszlopokat, és előbb próbaként futtatja le, hogy lássa a hibákat. Innen exportálhatja az adatokat is.",
    needs: "import",
  },
  {
    route: { view: "settings", tab: "users" },
    target: "nav-settings",
    title: "Kollégák és irodák meghívása",
    body: "A Beállítások › Felhasználók alatt hívhat meg kollégákat e-mailben, szerepkörrel: ügyintéző, pénzügyes, pénztáros, temetői dolgozó, olvasó vagy meghívott temetkezési iroda. Itt adja meg az üzemeltető adatait, a díjtételeket és a nyilvános sírkeresőt is.",
    needs: "manage_users",
  },
  {
    route: { view: "dashboard" },
    target: "search",
    title: "Gyors keresés",
    body: "Bármikor kereshet névre, évszámra vagy helyazonosítóra (például A/3/12). Billentyűzetről a Ctrl+K nyitja meg.",
  },
  {
    route: { view: "help" },
    target: "help",
    title: "Kész is!",
    body: "A Súgó oldalon részletes leírásokat talál minden feladathoz, és innen indíthatja újra ezt a bemutatót. Jó munkát kívánunk!",
  },
];

type Rect = { top: number; left: number; width: number; height: number };

export function Tour() {
  const { nav, can, status } = useApp();
  const steps = STEPS.filter((s) => !s.needs || can(s.needs));
  const [i, setI] = useState<number | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);

  // start: after the first-run wizard, or from the Súgó page
  useEffect(() => {
    const start = () => setI(0);
    window.addEventListener(TOUR_EVENT, start);
    if (localStorage.getItem(TOUR_KEY) === "start" && status?.session?.role !== "undertaker") {
      localStorage.setItem(TOUR_KEY, "seen");
      setI(0);
    }
    return () => window.removeEventListener(TOUR_EVENT, start);
  }, [status?.session?.role]);

  const step = i === null ? null : steps[i];

  // open the page of the step, then find the element (it may render late)
  useEffect(() => {
    if (!step) return;
    setRect(null);
    if (step.route) nav.go(step.route);
    if (!step.target) return;
    let tries = 0;
    let el: Element | null = null;
    const find = window.setInterval(() => {
      el = document.querySelector(`[data-tour="${step.target}"]`);
      tries += 1;
      if (el || tries > 25) {
        window.clearInterval(find);
        if (el) {
          el.scrollIntoView({ block: window.innerWidth < 640 ? "start" : "center", inline: "center" });
          const r = el.getBoundingClientRect();
          setRect({ top: r.top, left: r.left, width: r.width, height: r.height });
        }
      }
    }, 120);
    const onMove = () => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      setRect({ top: r.top, left: r.left, width: r.width, height: r.height });
    };
    window.addEventListener("resize", onMove);
    window.addEventListener("scroll", onMove, true);
    return () => {
      window.clearInterval(find);
      window.removeEventListener("resize", onMove);
      window.removeEventListener("scroll", onMove, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [i]);

  const close = useCallback(() => {
    localStorage.setItem(TOUR_KEY, "seen");
    setI(null);
  }, []);

  useEffect(() => {
    if (i === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
      if (e.key === "ArrowRight") setI((x) => (x !== null && x < steps.length - 1 ? x + 1 : x));
      if (e.key === "ArrowLeft") setI((x) => (x !== null && x > 0 ? x - 1 : x));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [i, steps.length, close]);

  const cardRef = useRef<HTMLDivElement>(null);
  const [card, setCard] = useState<{ top: number; left: number } | null>(null);
  const phone = typeof window !== "undefined" && window.innerWidth < 640;
  useLayoutEffect(() => {
    if (!rect || phone) {
      setCard(null);
      return;
    }
    // below the element when it fits, otherwise above; always on screen
    const w = Math.min(380, window.innerWidth - 24);
    const h = cardRef.current?.offsetHeight ?? 240;
    const below = rect.top + rect.height + 12;
    let top = below + h + 12 <= window.innerHeight ? below : rect.top - h - 12;
    top = Math.min(Math.max(12, top), window.innerHeight - h - 12);
    const left = Math.min(Math.max(12, rect.left), window.innerWidth - w - 12);
    setCard({ top, left });
  }, [rect, phone, i]);

  if (!step || i === null) return null;
  const last = i === steps.length - 1;
  const pad = 6;
  return createPortal(
    <div className="no-print fixed inset-0 z-[60]" role="dialog" aria-modal="true" aria-labelledby="tour-title" data-testid="tour">
      {rect ? (
        <div
          className="pointer-events-none fixed rounded-lg ring-3 ring-accent transition-all duration-200"
          style={{ top: rect.top - pad, left: rect.left - pad, width: rect.width + 2 * pad, height: rect.height + 2 * pad, boxShadow: "0 0 0 9999px rgba(22,24,29,0.55)" }}
        />
      ) : (
        <div className="fixed inset-0 bg-[#16181d]/55" />
      )}
      <div
        ref={cardRef}
        className="fixed max-h-[calc(100vh-24px)] w-[min(380px,calc(100vw-24px))] overflow-y-auto rounded-xl border border-seam bg-slab p-5 text-slate shadow-2xl"
        style={
          phone
            ? { bottom: 12, left: 12, right: 12, width: "auto", maxHeight: "55vh" }
            : card
              ? { top: card.top, left: card.left }
              : { top: "50%", left: "50%", transform: "translate(-50%, -50%)" }
        }
      >
        <p className="text-xs font-semibold tracking-wide text-granite uppercase">
          {i + 1} / {steps.length}
        </p>
        <h2 id="tour-title" className="mt-1 text-lg">
          {step.title}
        </h2>
        <p className="mt-2 text-[0.95rem] leading-relaxed">{step.body}</p>
        <div className="mt-4 flex items-center justify-between gap-2">
          <Button variant="quiet" onClick={close}>
            {last ? "Bezárás" : "Kihagyom"}
          </Button>
          <div className="flex gap-2">
            {i > 0 && <Button onClick={() => setI(i - 1)}>Vissza</Button>}
            <Button variant="primary" onClick={() => (last ? close() : setI(i + 1))} autoFocus>
              {last ? "Befejezés" : "Tovább"}
            </Button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
