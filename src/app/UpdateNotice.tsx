import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw, X } from "lucide-react";

const CHECK_EVERY_MS = 5 * 60 * 1000;

/**
 * A new version deployed while the program is open: a notice bottom right
 * offers to reload. Detected by comparing the build id with version.json
 * (every 5 minutes and when the tab comes back), or by a page part of the
 * old version that can no longer be loaded.
 */
export function UpdateNotice() {
  const { t } = useTranslation();
  const [available, setAvailable] = useState(false);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    // the development server has no version.json
    if (import.meta.env.DEV) return;
    let stop = false;
    const check = async () => {
      if (stop || document.visibilityState !== "visible") return;
      try {
        const r = await fetch(`${import.meta.env.BASE_URL}version.json?t=${Date.now()}`, { cache: "no-store" });
        if (!r.ok) return;
        const v = (await r.json()) as { build?: string };
        if (v.build && v.build !== __APP_BUILD__) setAvailable(true);
      } catch {
        // offline: try again later
      }
    };
    const timer = window.setInterval(check, CHECK_EVERY_MS);
    const onVisible = () => void check();
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    // a lazily loaded part of the old version is gone after a deployment
    const onPreloadError = (e: Event) => {
      e.preventDefault();
      setAvailable(true);
      setDismissed(false);
    };
    window.addEventListener("vite:preloadError", onPreloadError);
    return () => {
      stop = true;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
      window.removeEventListener("vite:preloadError", onPreloadError);
    };
  }, []);

  if (!available || dismissed) return null;
  return (
    <div
      role="status"
      aria-live="polite"
      data-testid="update-notice"
      className="no-print fixed right-4 bottom-4 z-50 flex max-w-sm items-start gap-3 border border-seam border-l-[6px] border-l-accent bg-slab px-4 py-3 shadow-lg"
    >
      <RefreshCw className="mt-0.5 size-5 shrink-0 text-moss" aria-hidden />
      <div className="flex flex-col gap-2">
        <p className="font-semibold">{t("update.title")}</p>
        <p className="text-sm text-granite">{t("update.body")}</p>
        <div>
          <button
            type="button"
            className="inline-flex min-h-10 items-center rounded-lg border border-moss bg-moss px-4 font-semibold text-moss-ink hover:bg-moss/90"
            onClick={() => window.location.reload()}
          >
            {t("update.reload")}
          </button>
        </div>
      </div>
      <button type="button" className="text-granite hover:text-ink" aria-label={t("common.close")} onClick={() => setDismissed(true)}>
        <X className="size-4" aria-hidden />
      </button>
    </div>
  );
}
