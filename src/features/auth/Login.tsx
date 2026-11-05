import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../../app/context";
import { shell } from "../../lib/api";
import { Alert, Button, ErrorAlert, TextField } from "../../components/ui";
import { RestoreForm } from "../settings/BackupTab";
import mark from "../../assets/mark.svg";

function Frame({ children }: { children: React.ReactNode }) {
  const { t } = useTranslation();
  return (
    <div className="grid min-h-full lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]">
      <aside className="flex flex-col justify-between bg-rail px-10 py-10 text-rail-ink">
        <p className="flex items-center gap-3">
          <img src={mark} alt="" className="size-10" />
          <span className="text-3xl font-extrabold tracking-[-0.03em]">{t("app.name")}</span>
        </p>
        <p className="mt-10 max-w-sm text-lg leading-snug text-rail-muted" aria-label={t("app.tagline")}>
          <Acronym />
        </p>
      </aside>
      <div className="flex items-center px-6 py-10 lg:px-16">
        <div className="w-full max-w-lg">{children}</div>
      </div>
    </div>
  );
}

/** "TEMetőmenedzsment és Informatikai SZoftver" with the letters of the name lit. */
export function Acronym() {
  return (
    <span aria-hidden>
      <b className="text-accent">Tem</b>etőmenedzsment és <b className="text-accent">I</b>nformatikai <b className="text-accent">Sz</b>oftver
    </span>
  );
}

export function Login() {
  const { t } = useTranslation();
  const { setStatus, sessionExpired, setSessionExpired } = useApp();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      setStatus(await shell.login(username, password));
      setSessionExpired(false);
    } catch (err) {
      setError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Frame>
      <h1 className="text-2xl">{t("login.title")}</h1>
      <p className="mt-1 mb-5 text-granite">{t("login.intro")}</p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        {sessionExpired && <Alert tone="warn">{t("errors.session_expired")}</Alert>}
        <TextField label={t("fields.username")} value={username} onChange={setUsername} inputProps={{ autoFocus: true, autoComplete: "username", name: "username" }} />
        <TextField label={t("fields.password")} type="password" value={password} onChange={setPassword} inputProps={{ autoComplete: "current-password", name: "password" }} />
        <ErrorAlert error={error} />
        <Button type="submit" variant="primary" size="lg" busy={busy}>
          {t("login.submit")}
        </Button>
      </form>
    </Frame>
  );
}

export function FirstRun() {
  const { t } = useTranslation();
  const { setStatus } = useApp();
  const [displayName, setDisplayName] = useState("");
  const [username, setUsername] = useState("");
  const [p1, setP1] = useState("");
  const [p2, setP2] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [restore, setRestore] = useState(false);
  const mismatch = p2.length > 0 && p1 !== p2;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (p1 !== p2) return;
    setBusy(true);
    setError(null);
    try {
      setStatus(await shell.setupAdmin(username, displayName, p1));
    } catch (err) {
      setError(err);
    } finally {
      setBusy(false);
    }
  };
  if (restore) {
    return (
      <Frame>
        <RestoreForm fresh onDone={(s) => s && setStatus(s)} onCancel={() => setRestore(false)} />
      </Frame>
    );
  }
  return (
    <Frame>
      <h1 className="text-2xl">{t("login.firstRunTitle")}</h1>
      <p className="mt-1 mb-5 text-granite">{t("login.firstRunIntro")}</p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <TextField label={t("fields.displayName")} value={displayName} onChange={setDisplayName} required inputProps={{ autoFocus: true }} />
        <TextField label={t("fields.username")} value={username} onChange={setUsername} required inputProps={{ autoComplete: "username" }} />
        <TextField label={t("fields.password")} type="password" value={p1} onChange={setP1} required hint={t("errors.password_too_short")} inputProps={{ autoComplete: "new-password" }} />
        <TextField label={t("login.passwordAgain")} type="password" value={p2} onChange={setP2} required error={mismatch ? t("login.passwordMismatch") : null} inputProps={{ autoComplete: "new-password" }} />
        <ErrorAlert error={error} />
        <Button type="submit" variant="primary" size="lg" busy={busy} disabled={!username || !displayName || p1.length < 8 || p1 !== p2}>
          {t("login.createAdmin")}
        </Button>
        <Button variant="quiet" onClick={() => setRestore(true)}>
          {t("login.restoreInstead")}
        </Button>
      </form>
    </Frame>
  );
}

export function DbProblem() {
  const { t } = useTranslation();
  const { status, setStatus } = useApp();
  const text = status?.dbError ? t(`errors.${status.dbError.code}`, { defaultValue: status.dbError.message }) : "";
  return (
    <Frame>
      <h1 className="text-2xl">{t("login.dbProblem")}</h1>
      <div className="mt-4 mb-6">
        <Alert tone="error">{text}</Alert>
      </div>
      <RestoreForm fresh onDone={(s) => s && setStatus(s)} />
    </Frame>
  );
}
