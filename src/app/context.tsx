import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { shell, setSessionExpiredHandler } from "../lib/api";
import type { AppStatus, Permission } from "../lib/types";

// ---------------------------------------------------------------- navigation
// A desktop app needs no URLs: a small view stack gives "back" behaviour.

export type Route =
  | { view: "dashboard" }
  | { view: "burials" }
  | { view: "newBurial"; plotId?: string; eventType?: string; bookingId?: string }
  | { view: "plots"; sectionId?: string }
  | { view: "plot"; id: string }
  | { view: "map"; plotId?: string }
  | { view: "persons" }
  | { view: "person"; id: string }
  | { view: "deceased"; id: string }
  | { view: "expiries" }
  | { view: "prints" }
  | { view: "reports" }
  | { view: "importExport" }
  | { view: "quickEntry" }
  | { view: "settings"; tab?: string }
  | { view: "account" }
  | { view: "platform" }
  | { view: "finance"; tab?: string }
  | { view: "operations" }
  | { view: "help" };

interface Nav {
  route: Route;
  go: (r: Route) => void;
  back: () => void;
  canGoBack: boolean;
}

interface Ctx {
  status: AppStatus | null;
  refresh: () => Promise<AppStatus>;
  setStatus: (s: AppStatus) => void;
  /** may do it here and now (false for writes while the current cemetery is read only) */
  can: (p: Permission) => boolean;
  /** has the permission at all, whatever the current cemetery's state */
  has: (p: Permission) => boolean;
  /** the current cemetery has no usable subscription: read only */
  cemeteryLocked: boolean;
  theme: string;
  zoom: number;
  setAppearance: (v: { theme?: string; zoom?: number }) => void;
  nav: Nav;
  cemeteryId: string | null;
  setCemeteryId: (id: string | null) => void;
  sessionExpired: boolean;
  setSessionExpired: (v: boolean) => void;
}

const AppCtx = createContext<Ctx | null>(null);

/** What stays possible while the current cemetery is read only: reading,
 *  printing, and the organisation's own settings (to subscribe, add a cemetery). */
const LOCKED_KEEPS: Permission[] = ["view_personal", "print", "export", "manage_settings", "manage_users", "manage_structure", "backup"];

export function AppProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [stack, setStack] = useState<Route[]>([{ view: "dashboard" }]);
  const [cemeteryId, setCemeteryIdState] = useState<string | null>(() => localStorage.getItem("lunda.cemetery"));
  const [sessionExpired, setSessionExpired] = useState(false);

  const refresh = useCallback(async () => {
    const s = await shell.status();
    setStatus(s);
    return s;
  }, []);

  useEffect(() => {
    void refresh();
    setSessionExpiredHandler(() => {
      setSessionExpired(true);
      void refresh();
    });
  }, [refresh]);

  // theme & zoom (NFR-HAS-04): each person's own choice, kept in the browser
  const [theme, setTheme] = useState(() => localStorage.getItem("lunda.theme") ?? "light");
  const [zoom, setZoom] = useState(() => Number(localStorage.getItem("lunda.zoom")) || 100);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.setProperty("--zoom", String(zoom / 100));
    localStorage.setItem("lunda.theme", theme);
    localStorage.setItem("lunda.zoom", String(zoom));
  }, [theme, zoom]);
  const setAppearance = useCallback((v: { theme?: string; zoom?: number }) => {
    if (v.theme) setTheme(v.theme);
    if (v.zoom) setZoom(v.zoom);
  }, []);

  const setCemeteryId = useCallback((id: string | null) => {
    setCemeteryIdState(id);
    if (id) localStorage.setItem("lunda.cemetery", id);
    else localStorage.removeItem("lunda.cemetery");
  }, []);

  const nav = useMemo<Nav>(
    () => ({
      route: stack[stack.length - 1],
      go: (r) => {
        setStack((s) => [...s.slice(-30), r]);
        window.scrollTo({ top: 0 });
      },
      back: () => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s)),
      canGoBack: stack.length > 1,
    }),
    [stack],
  );

  const has = useCallback((p: Permission) => !!status?.session?.permissions.includes(p), [status]);
  // a cemetery without a usable subscription is read only: the write buttons
  // are not offered (the server refuses the writes anyway)
  const cemeteryLocked = !!cemeteryId && !!status?.account?.lockedCemeteries?.includes(cemeteryId);
  const can = useCallback((p: Permission) => has(p) && (!cemeteryLocked || LOCKED_KEEPS.includes(p)), [has, cemeteryLocked]);

  return (
    <AppCtx.Provider
      value={{ status, refresh, setStatus, can, has, cemeteryLocked, theme, zoom, setAppearance, nav, cemeteryId, setCemeteryId, sessionExpired, setSessionExpired }}
    >
      {children}
    </AppCtx.Provider>
  );
}

export function useApp() {
  const c = useContext(AppCtx);
  if (!c) throw new Error("AppProvider missing");
  return c;
}

