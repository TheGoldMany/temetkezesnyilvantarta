import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CreditCard } from "lucide-react";
import { command } from "../../lib/api";
import { Alert, Button, Checkbox, ErrorAlert, Panel, Spinner, TextField } from "../../components/ui";

interface Settings {
  configured: boolean;
  payee: string;
  live: boolean;
  termsUrl: string;
  updatedAt: string | null;
}

/** The operator's own Barion shop: payers can pay invoices online by card. */
export function OnlinePayTab() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["online-pay"], queryFn: () => command<Settings>("online_pay_get") });
  const [form, setForm] = useState<{ posKey: string; payee: string; termsUrl: string; live: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>(null);
  if (!q.data) return q.error ? <ErrorAlert error={q.error} /> : <Spinner />;
  const d = q.data;
  const f = form ?? { posKey: "", payee: d.payee, termsUrl: d.termsUrl, live: d.live };
  const set = (patch: Partial<typeof f>) => (setSaved(false), setForm({ ...f, ...patch }));
  const save = async (remove = false) => {
    setBusy(true);
    setError(null);
    try {
      const next = await command<Settings>("online_pay_save", remove ? { remove: true } : f);
      qc.setQueryData(["online-pay"], next);
      setForm(null);
      setSaved(true);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Panel title={t("onlinePay.title")}>
      <div className="flex max-w-3xl flex-col gap-5">
        <p>{t("onlinePay.intro")}</p>
        <ol className="flex list-decimal flex-col gap-1 pl-5 text-granite">
          <li>{t("onlinePay.step1")}</li>
          <li>{t("onlinePay.step2")}</li>
          <li>{t("onlinePay.step3")}</li>
        </ol>
        {d.configured && (
          <Alert tone="ok">
            <span className="inline-flex items-center gap-2">
              <CreditCard className="size-4" aria-hidden />
              {d.live ? t("onlinePay.onLive", { payee: d.payee }) : t("onlinePay.onTest", { payee: d.payee })}
            </span>
          </Alert>
        )}
        <TextField
          label={t("onlinePay.posKey")}
          hint={d.configured ? t("onlinePay.posKeyKeep") : t("onlinePay.posKeyHint")}
          type="password"
          inputProps={{ name: "posKey", autoComplete: "off" }}
          value={f.posKey}
          onChange={(v) => set({ posKey: v })}
        />
        <TextField label={t("onlinePay.payee")} hint={t("onlinePay.payeeHint")} type="email" inputProps={{ name: "payee" }} value={f.payee} onChange={(v) => set({ payee: v })} />
        <TextField label={t("onlinePay.termsUrl")} hint={t("onlinePay.termsHint")} inputProps={{ name: "termsUrl", placeholder: "https://" }} value={f.termsUrl} onChange={(v) => set({ termsUrl: v })} />
        <Checkbox label={t("onlinePay.live")} hint={t("onlinePay.liveHint")} checked={f.live} onChange={(v) => set({ live: v })} />
        <ErrorAlert error={error} />
        {saved && <Alert tone="ok">{t("onlinePay.saved")}</Alert>}
        <div className="flex flex-wrap gap-3">
          <Button variant="primary" busy={busy} onClick={() => save()}>
            {t("onlinePay.save")}
          </Button>
          {d.configured && (
            <Button variant="quiet" disabled={busy} onClick={() => window.confirm(t("onlinePay.removeConfirm")) && save(true)}>
              {t("onlinePay.remove")}
            </Button>
          )}
        </div>
      </div>
    </Panel>
  );
}
