import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { HardDriveDownload, ShieldCheck } from "lucide-react";
import { useApp } from "../../app/context";
import { api, shell } from "../../lib/api";
import type { AppStatus } from "../../lib/types";
import { Alert, Button, Checkbox, Dialog, ErrorAlert, Panel, Spinner, TextField, useErrorText } from "../../components/ui";
import { fmtDateTime } from "../../lib/format";
import { useInvalidate } from "../../lib/queries";
import { useToast } from "../../app/toast";

interface Manifest {
  createdAt: string;
  kind: string;
  fileName: string;
  sizeBytes: number;
  sha256: string;
  appVersion: string;
}
interface BackupInfo {
  path: string;
  manifest: Manifest;
}
interface LogRow {
  ts: string;
  kind: string;
  path: string;
  sha256: string;
  external: boolean;
  ok: boolean;
  message: string;
}

/** FR-NAP-03..06 */
export function BackupTab() {
  const { t } = useTranslation();
  const { can, refresh } = useApp();
  const toast = useToast();
  const errText = useErrorText();
  const invalidate = useInvalidate();
  const settings = useQuery({ queryKey: ["settings"], queryFn: () => api<Record<string, string>>("settings.all") });
  const list = useQuery({ queryKey: ["backups"], queryFn: () => api<BackupInfo[]>("backup.list") });
  const log = useQuery({ queryKey: ["backupLog"], queryFn: () => api<LogRow[]>("backup.log") });
  const [dir, setDir] = useState("");
  const [keep, setKeep] = useState("30");
  const [onExit, setOnExit] = useState(true);
  const [busy, setBusy] = useState(false);
  const [key, setKey] = useState<string | null>(null);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [verifyPath, setVerifyPath] = useState<string | null>(null);
  useEffect(() => {
    if (settings.data) {
      setDir(settings.data["backup.directory"] ?? "");
      setKeep(settings.data["backup.keep_count"] ?? "30");
      setOnExit(settings.data["backup.on_exit"] !== "false");
    }
  }, [settings.data]);
  if (!settings.data) return <Spinner />;
  return (
    <div className="flex flex-col gap-6">
      <Panel title={t("backup.dir")}>
        <div className="flex max-w-2xl flex-col gap-4">
          <TextField label={t("backup.dir")} value={dir} onChange={setDir} hint={t("onboarding.backupDirHint")} />
          <TextField label={t("backup.keep")} type="number" value={keep} onChange={setKeep} />
          <Checkbox label={t("backup.onExit")} checked={onExit} onChange={setOnExit} />
          {can("manage_settings") && (
            <div>
              <Button
                variant="primary"
                onClick={async () => {
                  try {
                    await api("settings.set", { values: { "backup.directory": dir, "backup.keep_count": keep, "backup.on_exit": String(onExit) } });
                    await invalidate();
                    toast(t("common.savedToast"));
                  } catch (e) {
                    toast(errText(e), "error");
                  }
                }}
              >
                {t("common.saveChanges")}
              </Button>
            </div>
          )}
        </div>
      </Panel>
      <Panel
        title={t("backup.list")}
        actions={
          <>
            {can("backup") && (
              <Button
                variant="primary"
                busy={busy}
                icon={<HardDriveDownload className="size-5" aria-hidden />}
                onClick={async () => {
                  setBusy(true);
                  try {
                    const b = await api<BackupInfo>("backup.create");
                    toast(t("backup.done", { file: b.manifest.fileName }));
                    await invalidate();
                    await refresh();
                  } catch (e) {
                    toast(errText(e), "error");
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                {t("backup.now")}
              </Button>
            )}
            {can("manage_settings") && <Button onClick={async () => setKey(await api<string>("backup.recoveryKey"))}>{t("backup.showKey")}</Button>}
          </>
        }
      >
        {key && (
          <div className="mb-4">
            <Alert tone="warn" title={t("backup.recoveryKey")}>
              <span className="block font-mono text-lg break-all" data-testid="recovery-key">{key}</span>
            </Alert>
          </div>
        )}
        <ul className="divide-y divide-seam">
          {(list.data ?? []).map((b) => (
            <li key={b.path} className="flex flex-wrap items-center justify-between gap-3 py-3">
              <span className="min-w-0">
                <span className="font-semibold">{fmtDateTime(b.manifest.createdAt)}</span> · {t(`backup.kind.${b.manifest.kind}`, { defaultValue: b.manifest.kind })}
                <span className="block truncate text-sm text-granite">{b.path}</span>
                <span className="block font-mono text-xs text-granite">
                  {t("backup.checksum")}: {b.manifest.sha256.slice(0, 24)}…
                </span>
              </span>
              <span className="flex gap-2">
                <Button icon={<ShieldCheck className="size-4" aria-hidden />} onClick={() => setVerifyPath(b.path)}>
                  {t("backup.verify")}
                </Button>
                {can("manage_settings") && (
                  <Button variant="danger" onClick={() => setRestoring(b.path)}>
                    {t("backup.restore")}
                  </Button>
                )}
              </span>
            </li>
          ))}
        </ul>
      </Panel>
      <Panel title={t("backup.log")}>
        <ul className="flex flex-col gap-1 text-sm">
          {(log.data ?? []).map((l, i) => (
            <li key={i} className={l.ok ? "" : "text-alert"}>
              {fmtDateTime(l.ts)} · {t(`backup.kind.${l.kind}`, { defaultValue: l.kind })} · {l.external ? t("backup.external") : t("backup.local")} · {l.ok ? "✓" : `✗ ${l.message}`}
            </li>
          ))}
        </ul>
      </Panel>
      {verifyPath && <VerifyDialog path={verifyPath} onClose={() => setVerifyPath(null)} />}
      {restoring && (
        <Dialog open wide onClose={() => setRestoring(null)} title={t("backup.restoreTitle")}>
          <RestoreForm path={restoring} onDone={() => void refresh()} onCancel={() => setRestoring(null)} />
        </Dialog>
      )}
    </div>
  );
}

function VerifyDialog({ path, onClose }: { path: string; onClose: () => void }) {
  const { t } = useTranslation();
  const [key, setKey] = useState("");
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [error, setError] = useState<unknown>(null);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("backup.verify")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.close")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              setError(null);
              try {
                const r = await api<{ checksumOk: boolean; opened: boolean; integrityOk: boolean; auditChainOk: boolean; message: string }>("backup.verify", { path, recoveryKey: key || null });
                const ok = r.checksumOk && (!key || (r.opened && r.integrityOk && r.auditChainOk));
                setResult({ ok, message: r.message || (r.checksumOk ? "" : "checksum") });
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("backup.verify")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <p className="text-sm break-all text-granite">{path}</p>
        <TextField label={`${t("backup.recoveryKey")} (${t("common.optional")})`} value={key} onChange={setKey} />
        {result && (result.ok ? <Alert tone="ok">{t("backup.verifyOk")}</Alert> : <Alert tone="error">{t("backup.verifyFail", { message: t(`errors.${result.message}`, { defaultValue: result.message }) })}</Alert>)}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

/** Restore wizard (FR-NAP-05). `fresh` = before login on a new machine. */
export function RestoreForm({ path: initialPath, fresh, onDone, onCancel }: { path?: string; fresh?: boolean; onDone: (s?: AppStatus) => void; onCancel?: () => void }) {
  const { t } = useTranslation();
  const [path, setPath] = useState(initialPath ?? "");
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [done, setDone] = useState(false);
  const pick = async () => {
    if (!shell.isTauri) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const f = await open({ multiple: false, filters: [{ name: "Lunda mentés", extensions: ["ltbak"] }] });
    if (typeof f === "string") setPath(f);
  };
  if (done) return <Alert tone="ok">{t("backup.restored")}</Alert>;
  return (
    <div className="flex flex-col gap-4">
      <h2 className="text-xl">{t("backup.restoreTitle")}</h2>
      <p className="text-granite">{t("backup.restoreIntro")}</p>
      <div className="flex items-end gap-2">
        <TextField className="flex-1" label={t("backup.restorePath")} value={path} onChange={setPath} />
        {shell.isTauri && <Button onClick={pick}>…</Button>}
      </div>
      <TextField label={t("backup.recoveryKey")} value={key} onChange={setKey} hint="XXXXXXXX-XXXXXXXX-…" />
      <ErrorAlert error={error} />
      <div className="flex justify-end gap-3">
        {onCancel && <Button onClick={onCancel}>{t("common.cancel")}</Button>}
        <Button
          variant="danger"
          busy={busy}
          disabled={!path || key.replace(/[^0-9a-fA-F]/g, "").length !== 64}
          onClick={async () => {
            setBusy(true);
            setError(null);
            try {
              if (fresh) {
                onDone(await shell.restoreFresh(path, key));
              } else {
                await api("backup.restore", { path, recoveryKey: key });
                setDone(true);
                onDone();
              }
            } catch (e) {
              setError(e);
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("backup.restore")}
        </Button>
      </div>
    </div>
  );
}
