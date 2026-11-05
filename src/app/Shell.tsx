import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import {
  Home,
  BookOpenText,
  LayoutGrid,
  Map as MapIcon,
  Users,
  CalendarClock,
  Printer,
  BarChart3,
  ArrowDownUp,
  Settings,
  LogOut,
  Search,
  ArrowLeft,
} from "lucide-react";
import { useApp, type Route } from "./context";
import { api, shell } from "../lib/api";
import type { SearchHit } from "../lib/types";
import { cx, Plaque } from "../components/ui";
import { useCurrentCemetery } from "../lib/queries";

const NAV: { view: Route["view"]; key: string; icon: typeof Home }[] = [
  { view: "dashboard", key: "nav.dashboard", icon: Home },
  { view: "burials", key: "nav.burials", icon: BookOpenText },
  { view: "plots", key: "nav.plots", icon: LayoutGrid },
  { view: "map", key: "nav.map", icon: MapIcon },
  { view: "persons", key: "nav.persons", icon: Users },
  { view: "expiries", key: "nav.expiries", icon: CalendarClock },
  { view: "prints", key: "nav.prints", icon: Printer },
  { view: "reports", key: "nav.reports", icon: BarChart3 },
  { view: "importExport", key: "nav.importExport", icon: ArrowDownUp },
  { view: "settings", key: "nav.settings", icon: Settings },
];

const SECTION_OF: Partial<Record<Route["view"], Route["view"]>> = {
  newBurial: "burials",
  plot: "plots",
  person: "persons",
  deceased: "burials",
  quickEntry: "importExport",
};

export function Shell({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const { status, nav, setStatus } = useApp();
  const active = SECTION_OF[nav.route.view] ?? nav.route.view;
  const { current, list, setCemeteryId } = useCurrentCemetery();

  // keep the backend session alive while the user works; idle timeout is enforced in Rust
  useEffect(() => {
    let last = 0;
    const onAct = () => {
      const now = Date.now();
      if (now - last > 60_000) {
        last = now;
        void shell.touch().catch(() => {});
      }
    };
    window.addEventListener("keydown", onAct);
    window.addEventListener("pointerdown", onAct);
    return () => {
      window.removeEventListener("keydown", onAct);
      window.removeEventListener("pointerdown", onAct);
    };
  }, []);

  return (
    <div className="flex h-full">
      <aside className="no-print flex w-64 shrink-0 flex-col border-r border-seam bg-slab">
        <div className="px-5 pt-5 pb-4">
          <p className="font-serif text-xl font-semibold">{t("app.name")}</p>
          {list.length > 1 ? (
            <select
              aria-label={t("common.cemetery")}
              className="mt-2 w-full rounded-md border border-seam bg-limestone px-2 py-1 text-sm"
              value={current?.id ?? ""}
              onChange={(e) => setCemeteryId(e.target.value)}
            >
              {list.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          ) : (
            <p className="text-sm text-granite">{current?.name ?? t("app.tagline")}</p>
          )}
        </div>
        <nav aria-label="Fő menü" className="flex-1 overflow-y-auto px-3">
          <ul className="flex flex-col gap-0.5">
            {NAV.map((n) => {
              const Icon = n.icon;
              const on = active === n.view;
              return (
                <li key={n.view}>
                  <button
                    type="button"
                    onClick={() => nav.go({ view: n.view } as Route)}
                    aria-current={on ? "page" : undefined}
                    className={cx(
                      "flex min-h-12 w-full items-center gap-3 rounded-md px-3 text-left font-semibold",
                      on ? "bg-lichen text-slate" : "text-granite hover:bg-limestone hover:text-slate",
                    )}
                  >
                    <Icon className={cx("size-5", on && "text-moss")} aria-hidden />
                    {t(n.key)}
                  </button>
                </li>
              );
            })}
          </ul>
        </nav>
        <div className="border-t border-seam px-5 py-4">
          <p className="font-semibold">{status?.session?.displayName}</p>
          <p className="text-sm text-granite">{status?.session ? t(`role.${status.session.role}`) : ""}</p>
          {status?.loginRequired && (
            <button type="button" className="mt-2 inline-flex items-center gap-2 text-sm text-granite hover:text-slate" onClick={async () => setStatus(await shell.logout())}>
              <LogOut className="size-4" aria-hidden /> {t("nav.logout")}
            </button>
          )}
        </div>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="no-print flex items-center gap-3 border-b border-seam bg-limestone/90 px-8 py-3">
          {nav.canGoBack && (
            <button type="button" onClick={nav.back} className="inline-flex min-h-11 items-center gap-1 rounded-md px-2 text-granite hover:text-slate" aria-label={t("nav.back")}>
              <ArrowLeft className="size-5" aria-hidden />
            </button>
          )}
          <QuickSearch />
        </div>
        <main className="min-h-0 flex-1 overflow-y-auto px-8 py-7">
          <div className="mx-auto max-w-[1280px]">{children}</div>
        </main>
      </div>
    </div>
  );
}

/** FR-KER-01: quick search on every screen (Ctrl+K). */
function QuickSearch() {
  const { t } = useTranslation();
  const { nav } = useApp();
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const [debounced, setDebounced] = useState("");
  useEffect(() => {
    const h = setTimeout(() => setDebounced(q.trim()), 150);
    return () => clearTimeout(h);
  }, [q]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const { data = [], isFetching } = useQuery({
    queryKey: ["search", debounced],
    queryFn: () => api<SearchHit[]>("search.quick", { query: debounced, limit: 30 }),
    enabled: debounced.length >= 2,
  });
  const choose = (h: SearchHit) => {
    setOpen(false);
    setQ("");
    if (h.kind === "plot") nav.go({ view: "plot", id: h.id });
    else if (h.kind === "person") nav.go({ view: "person", id: h.id });
    else nav.go({ view: "deceased", id: h.id });
  };
  const groups: [string, SearchHit[]][] = [
    [t("search.deceased"), data.filter((h) => h.kind === "deceased")],
    [t("search.plots"), data.filter((h) => h.kind === "plot")],
    [t("search.persons"), data.filter((h) => h.kind === "person")],
  ];
  const flat = groups.flatMap(([, v]) => v);
  return (
    <div className="relative w-full max-w-2xl">
      <Search className="pointer-events-none absolute top-1/2 left-3 size-5 -translate-y-1/2 text-granite" aria-hidden />
      <input
        ref={inputRef}
        type="search"
        role="combobox"
        aria-expanded={open && debounced.length >= 2}
        aria-controls="quicksearch-results"
        aria-label={t("common.search")}
        placeholder={t("search.placeholder")}
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
          setActive(0);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((a) => Math.min(a + 1, flat.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((a) => Math.max(a - 1, 0));
          } else if (e.key === "Enter" && flat[active]) {
            e.preventDefault();
            choose(flat[active]);
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
        className="min-h-12 w-full rounded-md border border-seam bg-slab pr-24 pl-10 text-lg focus:border-moss focus:outline-none focus-visible:outline-3 focus-visible:outline-moss"
      />
      <kbd className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 rounded border border-seam px-1.5 text-sm text-granite">{t("search.shortcut")}</kbd>
      {open && debounced.length >= 2 && (
        <div id="quicksearch-results" role="listbox" className="absolute top-full right-0 left-0 z-40 mt-2 max-h-[70vh] overflow-y-auto rounded-lg border border-seam bg-slab py-2 shadow-lg">
          {flat.length === 0 && !isFetching && <p className="px-4 py-3 text-granite">{t("search.noResults", { q: debounced })}</p>}
          {groups.map(([label, hits]) =>
            hits.length ? (
              <div key={label} className="py-1">
                <p className="px-4 py-1 text-sm text-granite">{label}</p>
                {hits.map((h) => {
                  const idx = flat.indexOf(h);
                  return (
                    <button
                      key={h.kind + h.id}
                      type="button"
                      role="option"
                      aria-selected={idx === active}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => choose(h)}
                      onMouseEnter={() => setActive(idx)}
                      className={cx("flex w-full items-center justify-between gap-4 px-4 py-2 text-left", idx === active && "bg-lichen/70")}
                    >
                      <span className="min-w-0">
                        <span className="block font-semibold">{h.title}</span>
                        <span className="block text-sm text-granite">
                          {h.kind === "person" ? t("search.activeHolder", { count: Number(h.subtitle) }) : h.subtitle}
                          {h.cemeteryName ? ` · ${h.cemeteryName}` : ""}
                        </span>
                      </span>
                      {h.plotIdentifier && <Plaque identifier={h.plotIdentifier} />}
                    </button>
                  );
                })}
              </div>
            ) : null,
          )}
        </div>
      )}
    </div>
  );
}
