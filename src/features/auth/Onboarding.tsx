import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { Alert, Button, ErrorAlert, TextField, cx } from "../../components/ui";

/** First-start wizard (FR-BEA-04): operator, backup folder, recovery key, sample data. */
export function Onboarding() {
  const { t } = useTranslation();
  const { refresh } = useApp();
  const [step, setStep] = useState(0);
  const [values, setValues] = useState({ "operator.name": "", "operator.address": "", "signer.name": "", "signer.title": "temetőgondnok", "backup.directory": "" });
  const [key, setKey] = useState<string | null>(null);
  const [keyStored, setKeyStored] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [sample, setSample] = useState<{ plots: number; events: number } | null>(null);
  const steps = [t("onboarding.step1"), t("onboarding.step2"), t("onboarding.step3"), t("onboarding.step4")];
  const set = (k: keyof typeof values) => (v: string) => setValues((s) => ({ ...s, [k]: v }));

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  const next = () =>
    run(async () => {
      if (step === 0) await api("settings.set", { values: { "operator.name": values["operator.name"], "operator.address": values["operator.address"], "signer.name": values["signer.name"], "signer.title": values["signer.title"] } });
      if (step === 1) {
        await api("settings.set", { values: { "backup.directory": values["backup.directory"] } });
        setKey(await api<string>("backup.recoveryKey"));
      }
      setStep((s) => s + 1);
    });

  const finish = () =>
    run(async () => {
      await api("settings.set", { values: { "onboarding.completed": "true" } });
      await refresh();
    });

  return (
    <div className="flex min-h-full items-center justify-center px-6 py-10">
      <div className="w-full max-w-2xl">
        <p className="mb-2 font-serif text-3xl font-semibold">{t("onboarding.title")}</p>
        <ol className="mb-6 flex flex-wrap gap-2" aria-label={t("onboarding.title")}>
          {steps.map((s, i) => (
            <li key={s} aria-current={i === step ? "step" : undefined} className={cx("rounded-full border px-3 py-1 text-sm", i === step ? "border-moss bg-lichen font-semibold" : i < step ? "border-seam text-slate" : "border-seam text-granite")}>
              {i + 1}. {s}
            </li>
          ))}
        </ol>
        <div className="flex flex-col gap-4 rounded-lg border border-seam bg-slab px-8 py-7">
          <h1 className="text-2xl">{steps[step]}</h1>
          {step === 0 && (
            <>
              <p className="text-granite">{t("onboarding.step1Intro")}</p>
              <TextField label={t("settings.operatorName")} value={values["operator.name"]} onChange={set("operator.name")} inputProps={{ autoFocus: true }} />
              <TextField label={t("settings.operatorAddress")} value={values["operator.address"]} onChange={set("operator.address")} />
              <div className="grid grid-cols-2 gap-4">
                <TextField label={t("settings.signerName")} value={values["signer.name"]} onChange={set("signer.name")} />
                <TextField label={t("settings.signerTitle")} value={values["signer.title"]} onChange={set("signer.title")} />
              </div>
            </>
          )}
          {step === 1 && (
            <>
              <p className="text-granite">{t("onboarding.step2Intro")}</p>
              <TextField label={t("backup.dir")} value={values["backup.directory"]} onChange={set("backup.directory")} hint={t("onboarding.backupDirHint")} />
            </>
          )}
          {step === 2 && key && (
            <>
              <p className="text-granite">{t("onboarding.step3Intro")}</p>
              <p className="rounded-md border-2 border-dashed border-seam bg-limestone px-4 py-4 text-center font-mono text-lg tracking-wide break-all" data-testid="recovery-key">
                {key}
              </p>
              <div className="flex gap-3">
                <Button onClick={() => window.print()}>{t("onboarding.printKey")}</Button>
              </div>
              <label className="flex items-center gap-3 font-semibold">
                <input type="checkbox" className="size-5 accent-[var(--c-moss)]" checked={keyStored} onChange={(e) => setKeyStored(e.target.checked)} />
                {t("onboarding.keyStored")}
              </label>
            </>
          )}
          {step === 3 && (
            <>
              <p className="text-granite">{t("onboarding.step4Intro")}</p>
              {sample ? (
                <Alert tone="ok">{t("onboarding.sampleLoaded", sample)}</Alert>
              ) : (
                <div className="flex flex-wrap gap-3">
                  <Button
                    busy={busy}
                    onClick={() =>
                      run(async () => {
                        setSample(await api<{ plots: number; events: number }>("sample.load"));
                      })
                    }
                  >
                    {t("onboarding.loadSample")}
                  </Button>
                </div>
              )}
            </>
          )}
          <ErrorAlert error={error} />
          <div className="mt-2 flex justify-between gap-3">
            <Button variant="quiet" disabled={step === 0} onClick={() => setStep((s) => s - 1)}>
              {t("common.previous")}
            </Button>
            {step < 3 ? (
              <Button variant="primary" busy={busy} onClick={next} disabled={step === 2 && !keyStored}>
                {t("common.next")}
              </Button>
            ) : (
              <Button variant="primary" busy={busy} onClick={finish}>
                {sample ? t("onboarding.finish") : t("onboarding.startEmpty")}
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
