import { useTranslation } from "react-i18next";
import { Download } from "lucide-react";
import { useState } from "react";
import { api } from "../../lib/api";
import { Button, ErrorAlert, Panel } from "../../components/ui";
import { openGenerated, type GeneratedFile } from "../prints/PrintButton";
import { useApp } from "../../app/context";

/** Backups are made by the server; the customer can always download all data. */
export function BackupTab() {
  const { t } = useTranslation();
  const { can } = useApp();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  return (
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
                  await openGenerated(await api<GeneratedFile>("export.file", { kind: "full", format: "xlsx" }));
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
  );
}
