import { useTranslation } from "react-i18next";
import { Download } from "lucide-react";
import { useState } from "react";
import { api, command } from "../../lib/api";
import {
  Alert,
  Button,
  ErrorAlert,
  Panel,
  TextField,
} from "../../components/ui";
import { openGenerated, type GeneratedFile } from "../prints/PrintButton";
import { useApp } from "../../app/context";

/** Backups are made by the server; the customer can always download all data. */
export function BackupTab() {
  const { t } = useTranslation();
  const { can } = useApp();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  return (
    <div className="flex flex-col gap-6">
      <Panel title={t("backup.webTitle")}>
        <div className="flex max-w-2xl flex-col gap-4">
          <p>{t("backup.webIntro")}</p>
          <p className="text-granite">{t("backup.webExport")}</p>
          {can("export") && (
            <div>
              <Button
                variant="primary"
                busy={busy}
                icon={<Download className="size-5" aria-hidden />}
                onClick={async () => {
                  setBusy(true);
                  setError(null);
                  try {
                    await openGenerated(
                      await api<GeneratedFile>("export.file", {
                        kind: "full",
                        format: "xlsx",
                      }),
                    );
                  } catch (e) {
                    setError(e);
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                {t("backup.downloadAll")}
              </Button>
            </div>
          )}
          <ErrorAlert error={error} />
        </div>
      </Panel>
      {can("manage_settings") && <DesktopImport />}
    </div>
  );
}

/** Moving from the retired desktop app: its backup file plus the printed recovery key. */
function DesktopImport() {
  const { t } = useTranslation();
  const { refresh } = useApp();
  const [file, setFile] = useState<{ name: string; content: string } | null>(
    null,
  );
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [done, setDone] = useState<Record<string, number> | null>(null);
  return (
    <Panel title={t("backup.desktopTitle")}>
      <div className="flex max-w-2xl flex-col gap-4">
        <p>{t("backup.desktopIntro")}</p>
        {done ? (
          <Alert tone="ok" title={t("backup.desktopDone")}>
            {t("backup.desktopCounts", {
              plots: done.plots ?? 0,
              events: done.interment_events ?? 0,
              deceased: done.deceased ?? 0,
            })}
          </Alert>
        ) : (
          <>
            <label className="inline-flex w-fit cursor-pointer items-center gap-2 rounded-lg border border-seam bg-slab px-4 py-2 font-semibold">
              {file ? file.name : t("backup.pickDesktop")}
              <input
                type="file"
                accept=".ltbak"
                className="sr-only"
                data-testid="desktop-file"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (!f) return;
                  const r = new FileReader();
                  r.onload = () =>
                    setFile({
                      name: f.name,
                      content: String(r.result).split("base64,")[1] ?? "",
                    });
                  r.readAsDataURL(f);
                }}
              />
            </label>
            <TextField
              label={t("backup.recoveryKey")}
              value={key}
              onChange={setKey}
              hint={t("backup.recoveryKeyHint")}
              inputProps={{ name: "recoveryKey", autoComplete: "off" }}
            />
            <ErrorAlert error={error} />
            <div>
              <Button
                variant="primary"
                busy={busy}
                disabled={
                  !file || key.replace(/[^0-9a-fA-F]/g, "").length !== 64
                }
                onClick={async () => {
                  setBusy(true);
                  setError(null);
                  try {
                    const r = await command<{ counts: Record<string, number> }>(
                      "import_desktop_backup",
                      {
                        fileName: file!.name,
                        content: file!.content,
                        recoveryKey: key,
                      },
                    );
                    setDone(r.counts);
                    await refresh();
                  } catch (e) {
                    setError(e);
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                {t("backup.loadDesktop")}
              </Button>
            </div>
          </>
        )}
      </div>
    </Panel>
  );
}
