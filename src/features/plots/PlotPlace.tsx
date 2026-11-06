import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Camera, LocateFixed, Navigation, Paperclip } from "lucide-react";
import { api } from "../../lib/api";
import { useApp } from "../../app/context";
import { useInvalidate } from "../../lib/queries";
import { Button, ErrorAlert, Panel } from "../../components/ui";
import { uploadFile } from "../operations/Operations";

interface Attachment {
  id: string;
  fileName: string;
  mimeType: string;
  size: number;
  createdAt: string;
}

/** Opens a stored attachment in a new tab (photos, PDF) or downloads it. */
export async function openAttachment(id: string) {
  const f = await api<{ fileName: string; mimeType: string; data: string }>("attachments.get", { id });
  const bytes = Uint8Array.from(atob(f.data), (c) => c.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: f.mimeType }));
  const viewable = f.mimeType.startsWith("image/") || f.mimeType === "application/pdf";
  const tab = viewable ? window.open(url, "_blank") : null;
  if (!tab) {
    const a = document.createElement("a");
    a.href = url;
    a.download = f.fileName;
    document.body.appendChild(a);
    a.click();
    a.remove();
  }
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/** Where the grave is (GPS, route) and its photos and documents. */
export function PlotPlace({ plotId, lat, lon }: { plotId: string; lat: number | null; lon: number | null }) {
  const { t } = useTranslation();
  const { can } = useApp();
  const invalidate = useInvalidate();
  const files = useQuery({ queryKey: ["attachments", "plot", plotId], queryFn: () => api<Attachment[]>("attachments.list", { entity: "plot", entityId: plotId }) });
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [accuracy, setAccuracy] = useState<number | null>(null);
  const canPlace = can("edit_map") || can("manage_structure");
  const canAttach = can("operations");
  const here = () => {
    setError(null);
    if (!navigator.geolocation) {
      setError(new Error(t("place.noGps")));
      return;
    }
    setBusy(true);
    navigator.geolocation.getCurrentPosition(
      async (pos) => {
        try {
          await api("plots.setGps", { id: plotId, lat: Number(pos.coords.latitude.toFixed(6)), lon: Number(pos.coords.longitude.toFixed(6)) });
          setAccuracy(Math.round(pos.coords.accuracy));
          await invalidate();
        } catch (e) {
          setError(e);
        } finally {
          setBusy(false);
        }
      },
      (e) => {
        setBusy(false);
        setError(new Error(e.code === 1 ? t("place.denied") : t("place.noGps")));
      },
      { enableHighAccuracy: true, timeout: 20_000, maximumAge: 0 },
    );
  };
  return (
    <Panel title={t("place.title")}>
      <div className="flex flex-col gap-4" data-testid="plot-place">
        {lat != null && lon != null ? (
          <p className="tabular-nums">
            {t("place.coords", { lat: lat.toFixed(6), lon: lon.toFixed(6) })}
            {accuracy != null && <span className="block text-sm text-granite">{t("place.accuracy", { m: accuracy })}</span>}
          </p>
        ) : (
          <p className="text-granite">{t("place.none")}</p>
        )}
        <div className="flex flex-wrap gap-2">
          {lat != null && lon != null && (
            <a
              className="inline-flex min-h-11 items-center gap-2 rounded-lg border border-seam px-4 font-semibold hover:bg-lichen/40"
              href={`https://www.google.com/maps/dir/?api=1&destination=${lat},${lon}&travelmode=walking`}
              target="_blank"
              rel="noopener"
            >
              <Navigation className="size-4" aria-hidden />
              {t("place.route")}
            </a>
          )}
          {canPlace && (
            <Button icon={<LocateFixed className="size-4" aria-hidden />} busy={busy} onClick={here}>
              {t("place.here")}
            </Button>
          )}
        </div>
        {canPlace && <p className="text-sm text-granite">{t("place.hereHint")}</p>}

        <h3 className="font-semibold">{t("place.files")}</h3>
        {(files.data ?? []).length === 0 ? (
          <p className="text-granite">{t("place.noFiles")}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {files.data!.map((f) => (
              <li key={f.id}>
                <button type="button" className="text-left underline-offset-2 hover:underline" onClick={() => void openAttachment(f.id)}>
                  {f.mimeType.startsWith("image/") ? <Camera className="mr-1 inline size-4" aria-hidden /> : <Paperclip className="mr-1 inline size-4" aria-hidden />}
                  {f.fileName}
                </button>
              </li>
            ))}
          </ul>
        )}
        {canAttach && (
          <label className="inline-flex min-h-11 w-fit cursor-pointer items-center gap-2 rounded-lg border border-seam px-4 font-semibold hover:bg-lichen/40">
            <Camera className="size-4" aria-hidden />
            {t("place.addPhoto")}
            <input
              type="file"
              accept="image/*,application/pdf"
              capture="environment"
              className="sr-only"
              name="plotPhoto"
              onChange={async (e) => {
                const f = e.target.files?.[0];
                e.target.value = "";
                if (!f) return;
                setError(null);
                try {
                  await uploadFile("plot", plotId, f);
                  await invalidate();
                } catch (err) {
                  setError(err);
                }
              }}
            />
          </label>
        )}
        <ErrorAlert error={error} />
      </div>
    </Panel>
  );
}
