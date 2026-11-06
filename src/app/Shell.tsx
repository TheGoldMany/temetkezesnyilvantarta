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
  Building2,
  Wallet,
  ClipboardList,
} from "lucide-react";
import { useApp, type Route } from "./context";
import { api, shell } from "../lib/api";
import type { SearchHit } from "../lib/types";
import { cx, Plaque } from "../components/ui";
import { useCurrentCemetery } from "../lib/queries";
import { AccountBanners } from "../features/account/Security";

const NAV: { view: Route["view"]; key: string; icon: typeof Home }[] = [
  { view: "dashboard", key: "nav.dashboard", icon: Home },
  { view: "burials", key: "nav.burials", icon: BookOpenText },
  { view: "plots", key: "nav.plots", icon: LayoutGrid },
  { view: "map", key: "nav.map", icon: MapIcon },
  { view: "persons", key: "nav.persons", icon: Users },
  { view: "expiries", key: "nav.expiries", icon: CalendarClock },
  { view: "operations", key: "nav.operations", icon: ClipboardList },
  { view: "finance", key: "nav.finance", icon: Wallet },
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
  const { status, nav, setStatus, can } = useApp();
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
    <div className="flex h-full flex-col">
      {/* Top bar, as in the website's program window: identity, search, user; then the section tabs. */}
      <header className="no-print shrink-0 border-b border-seam bg-slab">
        <div className="flex h-16 items-center gap-3 px-4 sm:gap-5 sm:px-6">
          <div className="flex shrink-0 items-center gap-4">
            <p className="flex flex-col gap-0.5" title={t("app.tagline")}>
              <span className="wordmark text-[1.625rem] text-moss sm:text-[1.875rem]">{t("app.name")}</span>
              <span className="hidden text-[0.625rem] leading-none font-semibold tracking-wide text-granite sm:block">{t("app.tagline")}</span>
            </p>
            {list.length > 1 ? (
              <select
                aria-label={t("common.cemetery")}
                className="max-w-56 rounded-md border border-seam bg-slab px-2 py-1 text-sm text-granite"
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
              <span className="hidden max-w-64 truncate border-l border-seam pl-4 text-sm text-granite md:inline">{current?.name}</span>
            )}
          </div>
          {nav.canGoBack && (
            <button type="button" onClick={nav.back} className="inline-flex min-h-10 shrink-0 items-center gap-1 rounded-md px-2 text-granite hover:bg-limestone hover:text-slate" aria-label={t("nav.back")}>
              <ArrowLeft className="size-5" aria-hidden />
            </button>
          )}
          <div className="ml-auto flex min-w-0 flex-1 justify-end">
            {status?.session?.role !== "undertaker" && <QuickSearch />}
          </div>
          <div className="flex shrink-0 items-center gap-3 border-l border-seam pl-5">
            <span className="hidden text-right leading-tight lg:block">
              <span className="block text-sm font-semibold">{status?.session?.displayName}</span>
              <span className="block text-sm text-granite">{status?.session ? t(`role.${status.session.role}`) : ""}</span>
            </span>
            {status?.loginRequired && (
              <button
                type="button"
                className="inline-flex min-h-10 items-center gap-2 rounded-md px-2 text-sm text-granite hover:bg-limestone hover:text-slate"
                onClick={async () => setStatus(await shell.logout())}
                title={t("nav.logout")}
              >
                <LogOut className="size-5" aria-hidden />
                <span className="sr-only">{t("nav.logout")}</span>
              </button>
            )}
          </div>
        </div>
        <nav aria-label="Fő menü" className="scroll-thin overflow-x-auto px-4">
          <ul className="flex min-w-max">
            {[...NAV.filter((n) => status?.session?.role !== "undertaker" || n.view === "operations").filter((n) => (n.view !== "finance" || can("record_payments") || can("cash_desk")) && (n.view !== "operations" || can("operations") || can("complete_work") || can("calendar"))), ...(status?.account?.superadmin ? [{ view: "platform" as Route["view"], key: "nav.platform", icon: Building2 }] : [])].map((n) => {
              const Icon = n.icon;
              const on = active === n.view;
              return (
                <li key={n.view}>
                  <button
                    type="button"
                    onClick={() => nav.go({ view: n.view } as Route)}
                    aria-current={on ? "page" : undefined}
                    className={cx(
                      "flex h-12 items-center gap-2 px-3 text-[0.9375rem] font-medium whitespace-nowrap shadow-[inset_0_-3px_0_transparent] transition-colors",
                      on ? "text-slate shadow-[inset_0_-3px_0_var(--c-moss)] font-semibold" : "text-granite hover:text-slate",
                    )}
                  >
                    <Icon className={cx("size-[1.1rem]", on ? "text-moss" : "text-granite/80")} aria-hidden />
                    {t(n.key)}
                  </button>
                </li>
              );
            })}
          </ul>
        </nav>
      </header>
      <AccountBanners />
      <main className="min-h-0 flex-1 overflow-y-auto px-4 py-5 sm:px-6 sm:py-7 lg:px-10">
        <div className="mx-auto max-w-[1320px]">{children}</div>
      </main>
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
    <div className="relative w-full max-w-xl">
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
        className="min-h-11 w-full rounded-lg border border-transparent bg-limestone pr-3 pl-10 sm:pr-20 focus:border-moss focus:bg-slab focus:outline-none focus-visible:outline-3 focus-visible:outline-accent"
      />
      <kbd className="pointer-events-none absolute top-1/2 right-3 hidden -translate-y-1/2 rounded border sm:block border-seam bg-slab px-1.5 text-sm text-granite">{t("search.shortcut")}</kbd>
      {open && debounced.length >= 2 && (
        <div id="quicksearch-results" role="listbox" className="absolute top-full right-0 left-0 z-40 mt-2 max-h-[70vh] overflow-y-auto rounded-xl border border-seam bg-slab py-2 shadow-[0_20px_40px_-20px_rgba(20,59,49,0.35)]">
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
                          {h.cemeteryName ? `, ${h.cemeteryName}` : ""}
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
