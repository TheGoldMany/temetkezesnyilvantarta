import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { shell, setSessionExpiredHandler } from "../lib/api";
import type { AppStatus, Permission } from "../lib/types";

// ---------------------------------------------------------------- navigation
// A desktop app needs no URLs: a small view stack gives "back" behaviour.

export type Route =
  | { view: "dashboard" }
  | { view: "burials" }
  | { view: "newBurial"; plotId?: string; eventType?: string }
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
  | { view: "platform" };

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
  can: (p: Permission) => boolean;
  nav: Nav;
  cemeteryId: string | null;
  setCemeteryId: (id: string | null) => void;
  sessionExpired: boolean;
  setSessionExpired: (v: boolean) => void;
}

const AppCtx = createContext<Ctx | null>(null);

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

  // theme & zoom (NFR-HAS-04)
  useEffect(() => {
    const theme = status?.settings["ui.theme"] ?? localStorage.getItem("lunda.theme") ?? "light";
    const zoom = Number(status?.settings["ui.zoom"] ?? localStorage.getItem("lunda.zoom") ?? 100);
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.setProperty("--zoom", String((zoom || 100) / 100));
    localStorage.setItem("lunda.theme", theme);
    localStorage.setItem("lunda.zoom", String(zoom));
  }, [status?.settings]);

  const setCemeteryId = (id: string | null) => {
    setCemeteryIdState(id);
    if (id) localStorage.setItem("lunda.cemetery", id);
    else localStorage.removeItem("lunda.cemetery");
  };

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

  const can = useCallback((p: Permission) => !!status?.session?.permissions.includes(p), [status]);

  return (
    <AppCtx.Provider value={{ status, refresh, setStatus, can, nav, cemeteryId, setCemeteryId, sessionExpired, setSessionExpired }}>{children}</AppCtx.Provider>
  );
}

export function useApp() {
  const c = useContext(AppCtx);
  if (!c) throw new Error("AppProvider missing");
  return c;
}

export function useNav() {
  return useApp().nav;
}
