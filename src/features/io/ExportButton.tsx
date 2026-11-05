import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FileSpreadsheet } from "lucide-react";
import { api } from "../../lib/api";
import { Button, useErrorText } from "../../components/ui";
import { useToast } from "../../app/toast";
import { openGenerated, type GeneratedFile } from "../prints/PrintButton";

/** Generates an export file (XLSX/CSV/JSON) on the Rust side and opens it. */
export function ExportButton({ kind, args, label, format = "xlsx" }: { kind: string; args: Record<string, unknown>; label: string; format?: string }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const errText = useErrorText();
  return (
    <Button
      busy={busy}
      icon={<FileSpreadsheet className="size-5" aria-hidden />}
      onClick={async () => {
        setBusy(true);
        try {
          const f = await api<GeneratedFile>("export.file", { kind, format, args });
          await openGenerated(f);
          toast(t("prints.generated", { path: f.fileName }));
        } catch (e) {
          toast(errText(e), "error");
        } finally {
          setBusy(false);
        }
      }}
    >
      {label}
    </Button>
  );
}
