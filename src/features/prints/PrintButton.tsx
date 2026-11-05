import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Printer } from "lucide-react";
import { api } from "../../lib/api";
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
  /** file content (base64), sent inline by the server */
  data?: string;
}

/** Opens a generated PDF in a new tab, or downloads other files (XLSX, CSV). */
export async function openGenerated(f: GeneratedFile) {
  if (!f.data) return;
  const bytes = Uint8Array.from(atob(f.data), (c) => c.charCodeAt(0));
  const pdf = f.fileName.toLowerCase().endsWith(".pdf");
  const blob = new Blob([bytes], { type: pdf ? "application/pdf" : "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const tab = pdf ? window.open(url, "_blank") : null;
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
