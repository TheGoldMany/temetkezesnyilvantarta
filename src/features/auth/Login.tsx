import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../../app/context";
import { ApiError, isMfa, shell } from "../../lib/api";
import type { AppStatus, MfaChallenge } from "../../lib/types";
import { Alert, Button, ErrorAlert, TextField } from "../../components/ui";

function Frame({ children }: { children: React.ReactNode }) {
  const { t } = useTranslation();
  return (
    <div className="grid min-h-full lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]">
      <aside className="relative flex flex-col overflow-hidden bg-rail px-10 py-10 text-rail-ink">
        <p className="wordmark text-[2rem]">{t("app.name")}</p>
        <p className="mt-4 max-w-sm text-lg text-rail-muted">{t("app.tagline")}</p>
        <PlotPattern />
      </aside>
      <div className="flex items-center bg-limestone px-6 py-10 lg:px-16">
        <div className="w-full max-w-lg rounded-xl border border-seam bg-slab p-8 shadow-[0_20px_40px_-24px_rgba(20,59,49,0.35)]">{children}</div>
      </div>
    </div>
  );
}

/** A quiet parcel of plots, the same motif as the website's map. */
function PlotPattern() {
  const fills = ["#ffffff", "#7f9a8d", "#7f9a8d", "#7f9a8d", "#e2a32b", "#7f9a8d", "#ffffff", "#7f9a8d"];
  const cells = [];
  for (let r = 0; r < 6; r++) {
    for (let c = 0; c < 9; c++) {
      const f = fills[(r * 7 + c * 3 + ((r * c) % 5)) % fills.length];
      cells.push(<rect key={`${r}-${c}`} x={c * 30} y={r * 42} width={22} height={34} rx={4} fill={f} />);
    }
  }
  return (
    <svg viewBox="0 0 262 244" aria-hidden className="mt-auto hidden w-full max-w-sm opacity-30 lg:block">
      {cells}
    </svg>
  );
}

type Mode = "login" | "mfa" | "register" | "forgot" | "reset" | "invite";

/** Token from links in e-mails: #jelszo=… (new password), #meghivas=… (invitation). */
function linkToken(prefix: string): string | null {
  const h = window.location.hash;
  return h.startsWith(`#${prefix}=`) ? h.slice(prefix.length + 2) : null;
}

function clearHash() {
  history.replaceState(null, "", window.location.pathname + window.location.search);
}

export function Login() {
  const { setStatus } = useApp();
  const [mode, setMode] = useState<Mode>(() =>
    linkToken("jelszo") ? "reset" : linkToken("meghivas") ? "invite" : window.location.hash === "#regisztracio" ? "register" : "login",
  );
  const [challenge, setChallenge] = useState("");
  const [token, setToken] = useState(() => linkToken("jelszo") ?? linkToken("meghivas") ?? "");
  // links opened in a tab that already shows the login page only change the hash
  useEffect(() => {
    const onHash = () => {
      const reset = linkToken("jelszo");
      const invite = linkToken("meghivas");
      if (reset || invite) {
        setToken((reset ?? invite)!);
        setMode(reset ? "reset" : "invite");
      }
    };
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  const [notice, setNotice] = useState<string | null>(null);
  /** Login, reset and invitation answers: either signed in, or the second factor is needed. */
  const done = (r: AppStatus | MfaChallenge) => {
    clearHash();
    if (isMfa(r)) {
      setChallenge(r.challenge);
      setMode("mfa");
    } else {
      setStatus(r);
    }
  };
  const go = (m: Mode) => {
    clearHash();
    setNotice(null);
    setMode(m);
  };
  switch (mode) {
    case "mfa":
      return <MfaForm challenge={challenge} onChallenge={setChallenge} onBack={() => go("login")} />;
    case "register":
      return <RegisterForm onLogin={() => go("login")} />;
    case "forgot":
      return (
        <ForgotForm
          onSent={() => {
            setNotice("sent");
            setMode("login");
          }}
          onBack={() => go("login")}
        />
      );
    case "reset":
      return <NewPasswordForm token={token} invite={false} onDone={done} onBack={() => go("login")} />;
    case "invite":
      return <NewPasswordForm token={token} invite onDone={done} onBack={() => go("login")} />;
    default:
      return <LoginForm notice={notice} onDone={done} onRegister={() => go("register")} onForgot={() => go("forgot")} />;
  }
}

function SwitchLink({ text, action, onClick }: { text: string; action: string; onClick: () => void }) {
  return (
    <p className="mt-6 border-t border-seam pt-5 text-granite">
      {text}{" "}
      <button type="button" className="font-semibold text-moss underline underline-offset-3" onClick={onClick}>
        {action}
      </button>
    </p>
  );
}

function useSubmit(fn: () => Promise<void>) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const submit = async (e?: FormEvent) => {
    e?.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (err) {
      setError(err);
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, setError, submit };
}

function LoginForm({ notice, onDone, onRegister, onForgot }: { notice: string | null; onDone: (r: AppStatus | MfaChallenge) => void; onRegister: () => void; onForgot: () => void }) {
  const { t } = useTranslation();
  const { sessionExpired, setSessionExpired } = useApp();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const { busy, error, submit } = useSubmit(async () => {
    onDone(await shell.login(email, password));
    setSessionExpired(false);
  });
  return (
    <Frame>
      <h1 className="text-2xl">{t("login.title")}</h1>
      <p className="mt-1 mb-5 text-granite">{t("login.intro")}</p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        {sessionExpired && <Alert tone="warn">{t("errors.session_expired")}</Alert>}
        {notice === "sent" && <Alert tone="ok">{t("login.resetSent")}</Alert>}
        <TextField label={t("login.email")} type="email" value={email} onChange={setEmail} inputProps={{ autoFocus: true, autoComplete: "username", name: "email" }} />
        <TextField label={t("login.password")} type="password" value={password} onChange={setPassword} inputProps={{ autoComplete: "current-password", name: "password" }} />
        <ErrorAlert error={error} />
        <Button type="submit" variant="primary" size="lg" busy={busy}>
          {t("login.submit")}
        </Button>
        <button type="button" className="self-start text-moss underline underline-offset-3" onClick={onForgot}>
          {t("login.forgot")}
        </button>
      </form>
      <SwitchLink text={t("login.noAccount")} action={t("login.toRegister")} onClick={onRegister} />
    </Frame>
  );
}

function MfaForm({ challenge, onChallenge, onBack }: { challenge: string; onChallenge: (c: string) => void; onBack: () => void }) {
  const { t } = useTranslation();
  const { setStatus } = useApp();
  const [code, setCode] = useState("");
  const { busy, error, submit } = useSubmit(async () => {
    try {
      setStatus(await shell.loginMfa(challenge, code));
    } catch (e) {
      // the server issues a fresh challenge for the next try
      if (e instanceof ApiError && e.params.challenge) onChallenge(e.params.challenge);
      setCode("");
      throw e;
    }
  });
  return (
    <Frame>
      <h1 className="text-2xl">{t("login.mfaTitle")}</h1>
      <p className="mt-1 mb-5 text-granite">{t("login.mfaIntro")}</p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <TextField label={t("login.mfaCode")} value={code} onChange={setCode} hint={t("login.mfaHint")} inputProps={{ autoFocus: true, autoComplete: "one-time-code", inputMode: "numeric", name: "code" }} />
        <ErrorAlert error={error} />
        <Button type="submit" variant="primary" size="lg" busy={busy} disabled={code.trim().length < 6}>
          {t("login.submit")}
        </Button>
      </form>
      <SwitchLink text="" action={t("common.previous")} onClick={onBack} />
    </Frame>
  );
}

function ForgotForm({ onSent, onBack }: { onSent: () => void; onBack: () => void }) {
  const { t } = useTranslation();
  const [email, setEmail] = useState("");
  const { busy, error, submit } = useSubmit(async () => {
    await shell.forgotPassword(email);
    onSent();
  });
  return (
    <Frame>
      <h1 className="text-2xl">{t("login.forgotTitle")}</h1>
      <p className="mt-1 mb-5 text-granite">{t("login.forgotIntro")}</p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <TextField label={t("login.email")} type="email" value={email} onChange={setEmail} inputProps={{ autoFocus: true, autoComplete: "email", name: "email" }} />
        <ErrorAlert error={error} />
        <Button type="submit" variant="primary" size="lg" busy={busy} disabled={!email.includes("@")}>
          {t("login.sendLink")}
        </Button>
      </form>
      <SwitchLink text="" action={t("login.toLogin")} onClick={onBack} />
    </Frame>
  );
}

function NewPasswordForm({ token, invite, onDone, onBack }: { token: string; invite: boolean; onDone: (r: AppStatus | MfaChallenge) => void; onBack: () => void }) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [p1, setP1] = useState("");
  const [p2, setP2] = useState("");
  const mismatch = p2.length > 0 && p1 !== p2;
  const { busy, error, submit } = useSubmit(async () => {
    if (p1 !== p2) return;
    onDone(invite ? await shell.acceptInvite(token, name, p1) : await shell.resetPassword(token, p1));
  });
  return (
    <Frame>
      <h1 className="text-2xl">{invite ? t("login.inviteTitle") : t("login.resetTitle")}</h1>
      <p className="mt-1 mb-5 text-granite">{invite ? t("login.inviteIntro") : t("login.resetIntro")}</p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        {invite && <TextField label={t("login.name")} value={name} onChange={setName} hint={t("login.inviteNameHint")} inputProps={{ autoComplete: "name", name: "name" }} />}
        <TextField label={t("login.newPassword")} type="password" value={p1} onChange={setP1} required hint={t("login.passwordHint")} inputProps={{ autoFocus: !invite, autoComplete: "new-password", name: "password" }} />
        <TextField label={t("login.passwordAgain")} type="password" value={p2} onChange={setP2} required error={mismatch ? t("login.passwordMismatch") : null} inputProps={{ autoComplete: "new-password", name: "password2" }} />
        <ErrorAlert error={error} />
        <Button type="submit" variant="primary" size="lg" busy={busy} disabled={p1.length < 8 || p1 !== p2}>
          {invite ? t("login.inviteSubmit") : t("login.resetSubmit")}
        </Button>
      </form>
      <SwitchLink text="" action={t("login.toLogin")} onClick={onBack} />
    </Frame>
  );
}

function RegisterForm({ onLogin }: { onLogin: () => void }) {
  const { t } = useTranslation();
  const { setStatus } = useApp();
  const [organization, setOrganization] = useState("");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [p1, setP1] = useState("");
  const [p2, setP2] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const mismatch = p2.length > 0 && p1 !== p2;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (p1 !== p2) return;
    setBusy(true);
    setError(null);
    try {
      setStatus(await shell.register(organization, name, email, p1));
    } catch (err) {
      setError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Frame>
      <h1 className="text-2xl">{t("login.registerTitle")}</h1>
      <p className="mt-1 mb-5 text-granite">{t("login.registerIntro")}</p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <TextField label={t("login.organization")} hint={t("login.organizationHint")} value={organization} onChange={setOrganization} required inputProps={{ autoFocus: true, autoComplete: "organization", name: "organization" }} />
        <TextField label={t("login.name")} value={name} onChange={setName} required inputProps={{ autoComplete: "name", name: "name" }} />
        <TextField label={t("login.email")} type="email" value={email} onChange={setEmail} required inputProps={{ autoComplete: "email", name: "email" }} />
        <TextField label={t("login.password")} type="password" value={p1} onChange={setP1} required hint={t("login.passwordHint")} inputProps={{ autoComplete: "new-password", name: "password" }} />
        <TextField label={t("login.passwordAgain")} type="password" value={p2} onChange={setP2} required error={mismatch ? t("login.passwordMismatch") : null} inputProps={{ autoComplete: "new-password", name: "password2" }} />
        <ErrorAlert error={error} />
        <p className="text-sm text-granite">
          {t("login.termsBefore")}{" "}
          <a className="text-moss underline underline-offset-3" href="/aszf.html" target="_blank" rel="noreferrer">
            {t("login.termsLink")}
          </a>{" "}
          {t("login.termsAnd")}{" "}
          <a className="text-moss underline underline-offset-3" href="/adatfeldolgozas.html" target="_blank" rel="noreferrer">
            {t("login.dpaLink")}
          </a>
          .
        </p>
        <Button type="submit" variant="primary" size="lg" busy={busy} disabled={!organization || !name || !email || p1.length < 8 || p1 !== p2}>
          {t("login.createAccount")}
        </Button>
      </form>
      <SwitchLink text={t("login.haveAccount")} action={t("login.toLogin")} onClick={onLogin} />
    </Frame>
  );
}
