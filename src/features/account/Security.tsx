import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ShieldCheck } from "lucide-react";
import { useApp } from "../../app/context";
import { shell, type SecurityEvent } from "../../lib/api";
import { Alert, Button, ErrorAlert, Panel, TextField } from "../../components/ui";

/** Two-factor sign-in setup: QR code, first code, recovery codes. */
export function TotpSetup({ onDone }: { onDone?: () => void }) {
  const { t } = useTranslation();
  const { refresh } = useApp();
  const [setup, setSetup] = useState<{ secret: string; qrSvg: string } | null>(null);
  const [code, setCode] = useState("");
  const [codes, setCodes] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    shell.totpSetup().then(setSetup, setError);
  }, []);
  if (codes) {
    return (
      <div className="flex flex-col gap-4">
        <Alert tone="ok">{t("security.enabled")}</Alert>
        <p>{t("security.recoveryIntro")}</p>
        <ul className="grid grid-cols-2 gap-2 rounded-lg border border-dashed border-seam bg-limestone p-4 font-semibold tabular-nums" data-testid="recovery-codes">
          {codes.map((c) => (
            <li key={c}>{c.replace(/(.{5})/, "$1-")}</li>
          ))}
        </ul>
        <div className="flex flex-wrap gap-3">
          <Button onClick={() => window.print()}>{t("security.printCodes")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              await refresh();
              onDone?.();
            }}
          >
            {t("security.codesSaved")}
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-4">
      <ol className="flex list-decimal flex-col gap-2 pl-5">
        <li>{t("security.step1")}</li>
        <li>{t("security.step2")}</li>
        <li>{t("security.step3")}</li>
      </ol>
      {setup && (
        <div className="flex flex-wrap items-center gap-6">
          <div className="size-48 rounded-lg border border-seam bg-white p-2 [&>svg]:size-full" aria-label={t("security.qr")} dangerouslySetInnerHTML={{ __html: setup.qrSvg }} />
          <div className="min-w-0">
            <p className="text-sm text-granite">{t("security.manualKey")}</p>
            <p className="font-semibold tracking-wider break-all tabular-nums" data-testid="totp-secret">
              {setup.secret.replace(/(.{4})/g, "$1 ").trim()}
            </p>
          </div>
        </div>
      )}
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          setError(null);
          try {
            setCodes((await shell.totpEnable(code)).recoveryCodes);
          } catch (err) {
            setError(err);
          } finally {
            setBusy(false);
          }
        }}
      >
        <TextField className="w-48" label={t("login.mfaCode")} value={code} onChange={setCode} inputProps={{ inputMode: "numeric", autoComplete: "one-time-code", name: "code" }} />
        <Button type="submit" variant="primary" busy={busy} disabled={code.trim().length !== 6}>
          {t("security.enable")}
        </Button>
      </form>
      <ErrorAlert error={error} />
    </div>
  );
}

/** Full-screen step for admins who must turn on two-factor sign-in. */
export function TotpRequired() {
  const { t } = useTranslation();
  const { setStatus } = useApp();
  return (
    <div className="flex min-h-full items-center justify-center bg-limestone px-6 py-10">
      <div className="w-full max-w-2xl rounded-xl border border-seam bg-slab p-8 shadow-[0_20px_40px_-24px_rgba(20,59,49,0.35)]">
        <p className="wordmark mb-6 text-2xl text-moss">{t("app.name")}</p>
        <h1 className="text-2xl">{t("security.requiredTitle")}</h1>
        <p className="mt-1 mb-6 text-granite">{t("security.requiredIntro")}</p>
        <TotpSetup />
        <button type="button" className="mt-6 text-granite underline underline-offset-3" onClick={async () => setStatus(await shell.logout())}>
          {t("nav.logout")}
        </button>
      </div>
    </div>
  );
}

/** Settings panel: own sign-in security. */
export function SecurityPanel() {
  const { t } = useTranslation();
  const { status, refresh } = useApp();
  const acc = status?.account;
  const [setupOpen, setSetupOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [error, setError] = useState<unknown>(null);
  if (!acc) return null;
  return (
    <Panel title={t("security.title")}>
      <div className="flex max-w-2xl flex-col gap-4">
        <p className="flex items-center gap-2">
          <ShieldCheck className={acc.totpEnabled ? "size-5 text-moss" : "size-5 text-granite"} aria-hidden />
          {acc.totpEnabled ? t("security.on") : t("security.off")}
        </p>
        {!acc.totpEnabled && !setupOpen && (
          <div>
            <Button variant="primary" onClick={() => setSetupOpen(true)}>
              {t("security.turnOn")}
            </Button>
          </div>
        )}
        {!acc.totpEnabled && setupOpen && <TotpSetup onDone={() => setSetupOpen(false)} />}
        {acc.totpEnabled && (
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={async (e) => {
              e.preventDefault();
              setError(null);
              try {
                await shell.totpDisable(password);
                setPassword("");
                await refresh();
              } catch (err) {
                setError(err);
              }
            }}
          >
            <TextField className="w-64" label={t("security.passwordToDisable")} type="password" value={password} onChange={setPassword} />
            <Button type="submit" variant="danger" disabled={!password}>
              {t("security.turnOff")}
            </Button>
          </form>
        )}
        <ErrorAlert error={error} />
      </div>
    </Panel>
  );
}

/** Notices under the top bar: unconfirmed e-mail address, trial days left. */
export function AccountBanners() {
  const { t } = useTranslation();
  const { status, can, nav } = useApp();
  const [sent, setSent] = useState(false);
  const acc = status?.account;
  if (!acc) return null;
  return (
    <>
      {!acc.emailVerified && (
        <div className="no-print border-b border-accent/40 bg-lichen px-4 py-1.5 text-sm sm:px-6 sm:py-2" role="status">
          <span className="sm:hidden">{t("account.verifyShort")} </span>
          <span className="hidden sm:inline">{t("account.verifyNotice", { email: acc.email })} </span>
          {sent ? (
            <span className="font-semibold">{t("account.verifySent")}</span>
          ) : (
            <button
              type="button"
              className="font-semibold text-moss underline underline-offset-3"
              onClick={async () => {
                await shell.resendVerification().catch(() => {});
                setSent(true);
              }}
            >
              {t("account.resend")}
            </button>
          )}
        </div>
      )}
      {acc.tenantStatus === "trial" && (
        <div className="no-print border-b border-seam bg-slab px-4 py-1.5 text-sm text-granite sm:px-6 sm:py-2" role="status" data-testid="trial-banner">
          <span>{t("account.trialLeft", { count: acc.trialDaysLeft, date: acc.trialEndsAt })} </span>
          {can("manage_settings") && (
            <button type="button" className="font-semibold text-moss underline underline-offset-3" onClick={() => nav.go({ view: "settings", tab: "subscription" })}>
              {t("account.subscribe")}
            </button>
          )}
        </div>
      )}
      {acc.tenantStatus === "read_only" && (
        <div className="no-print border-b border-accent/40 bg-lichen px-4 py-1.5 text-sm font-semibold sm:px-6 sm:py-2" role="alert" data-testid="readonly-banner">
          <span>{t("account.readOnly")} </span>
          {can("manage_settings") && (
            <button type="button" className="text-moss underline underline-offset-3" onClick={() => nav.go({ view: "settings", tab: "subscription" })}>
              {t("account.subscribe")}
            </button>
          )}
        </div>
      )}
    </>
  );
}

/** Short browser and system name from a user agent string. */
function browserName(ua: string): string {
  const b = /Edg\//.test(ua) ? "Edge" : /Firefox\//.test(ua) ? "Firefox" : /Chrome\//.test(ua) ? "Chrome" : /Safari\//.test(ua) ? "Safari" : "";
  const os = /Android/.test(ua) ? "Android" : /iPhone|iPad/.test(ua) ? "iOS" : /Windows/.test(ua) ? "Windows" : /Mac OS X/.test(ua) ? "macOS" : /Linux/.test(ua) ? "Linux" : "";
  return [b, os].filter(Boolean).join(", ") || ua.slice(0, 40);
}

/** Administrators: sign-ins and security changes of the organisation's accounts. */
export function SecurityLogPanel() {
  const { t } = useTranslation();
  const [rows, setRows] = useState<SecurityEvent[] | null>(null);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    shell.securityLog().then(setRows, setError);
  }, []);
  return (
    <Panel title={t("securityLog.title")}>
      <p className="mb-3 max-w-2xl text-granite">{t("securityLog.intro")}</p>
      <ErrorAlert error={error} />
      {rows && rows.length === 0 && <p className="text-granite">{t("securityLog.empty")}</p>}
      {rows && rows.length > 0 && (
        <div className="overflow-x-auto">
          <table className="w-full text-sm" data-testid="security-log">
            <thead>
              <tr className="text-left text-granite">
                <th className="py-2 pr-4">{t("securityLog.at")}</th>
                <th className="py-2 pr-4">{t("securityLog.event")}</th>
                <th className="py-2 pr-4">{t("securityLog.user")}</th>
                <th className="py-2 pr-4">{t("securityLog.ip")}</th>
                <th className="py-2">{t("securityLog.browser")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr key={i} className="border-t border-seam">
                  <td className="py-2 pr-4 whitespace-nowrap tabular-nums">{new Date(r.at).toLocaleString("hu-HU")}</td>
                  <td className={r.kind.endsWith("_failed") ? "py-2 pr-4 font-semibold text-accent" : "py-2 pr-4"}>{t(`securityLog.kinds.${r.kind}`, r.kind)}</td>
                  <td className="py-2 pr-4 break-all">{r.email}</td>
                  <td className="py-2 pr-4 tabular-nums">{r.ip || "–"}</td>
                  <td className="py-2" title={r.userAgent}>
                    {browserName(r.userAgent)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  );
}
