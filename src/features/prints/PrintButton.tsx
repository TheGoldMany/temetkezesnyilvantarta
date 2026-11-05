import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Printer } from "lucide-react";
import { api, shell } from "../../lib/api";
import { Button } from "../../components/ui";
import { useToast } from "../../app/toast";
import { useErrorText } from "../../components/ui";

export interface GeneratedFile {
  path: string;
  fileName: string;
  pages: number;
  sha256: string;
  contentHash?: string | null;
  volumeId?: string | null;
}

/** Opens a generated file (PDF, XLSX…) with the system viewer, or in a browser tab during development. */
export async function openGenerated(f: GeneratedFile) {
  if (shell.isTauri) {
    const { openPath } = await import("@tauri-apps/plugin-opener");
    await openPath(f.path);
  } else {
    window.open(`/file?path=${encodeURIComponent(f.path)}`, "_blank");
  }
}

/** Generates a document on the Rust side (FR-NYO) and opens it. */
export function PrintButton({ doc, args, label, variant = "secondary" }: { doc: string; args: Record<string, unknown>; label: string; variant?: "primary" | "secondary" | "quiet" }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const errText = useErrorText();
  return (
    <Button
      variant={variant}
      busy={busy}
      icon={<Printer className="size-5" aria-hidden />}
      onClick={async () => {
        setBusy(true);
        try {
          const f = await api<GeneratedFile>("print.document", { doc, args });
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
