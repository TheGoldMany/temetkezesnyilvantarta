import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Upload } from "lucide-react";
import { command } from "../../lib/api";
import { Alert, Button, ErrorAlert, TextField } from "../../components/ui";

const ACCEPT = ".xlsx,.xls,.xlsm,.ods,.csv,.docx,.doc,.odt,.rtf,.txt,.pdf";
const MAX_BYTES = 3 * 1024 * 1024;

function toBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).split(",")[1] ?? "");
    r.onerror = () => reject(r.error);
    r.readAsDataURL(file);
  });
}

/**
 * The customer sends its current register (Excel, Word, CSV, PDF): the TEMISZ
 * team loads it into the organisation free of charge and writes when done.
 */
export function DataUpload({ compact }: { compact?: boolean }) {
  const { t } = useTranslation();
  const input = useRef<HTMLInputElement>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [sent, setSent] = useState<string | null>(null);

  const send = async (file: File) => {
    setError(null);
    if (file.size > MAX_BYTES) {
      setError(new Error(t("dataUpload.tooLarge")));
      return;
    }
    setBusy(true);
    try {
      await command("data_upload", { fileName: file.name, content: await toBase64(file), message });
      setSent(file.name);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };

  if (sent) {
    return (
      <div data-testid="data-upload-done">
        <Alert tone="ok">{t("dataUpload.sent", { file: sent })}</Alert>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-3" data-testid="data-upload">
      {!compact && <TextField label={t("dataUpload.message")} value={message} onChange={setMessage} hint={t("dataUpload.messageHint")} />}
      <input
        ref={input}
        type="file"
        accept={ACCEPT}
        className="sr-only"
        aria-label={t("dataUpload.choose")}
        data-testid="data-upload-file"
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) void send(f);
        }}
      />
      <div>
        <Button variant={compact ? "secondary" : "primary"} busy={busy} icon={<Upload className="size-4" aria-hidden />} onClick={() => input.current?.click()}>
          {t("dataUpload.choose")}
        </Button>
      </div>
      <p className="text-sm text-granite">{t("dataUpload.privacy")}</p>
      <ErrorAlert error={error} />
    </div>
  );
}
