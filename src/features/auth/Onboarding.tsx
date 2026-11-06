import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { Alert, Button, ErrorAlert, TextField, cx } from "../../components/ui";

/** First sign-in wizard (FR-BEA-04): operator details, then optional sample data. */
export function Onboarding() {
  const { t } = useTranslation();
  const { refresh, status } = useApp();
  const [step, setStep] = useState(0);
  const [values, setValues] = useState({
    "operator.name": status?.settings["operator.name"] ?? "",
    "operator.address": status?.settings["operator.address"] ?? "",
    "signer.name": status?.settings["signer.name"] || status?.session?.displayName || "",
    "signer.title": status?.settings["signer.title"] || "temetőgondnok",
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [sample, setSample] = useState<{ plots: number; events: number } | null>(null);
  const steps = [t("onboarding.step1"), t("onboarding.step4")];
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
      setStep((s) => s + 1);
    });

  const finish = () =>
    run(async () => {
      await api("settings.set", { values: { "onboarding.completed": "true" } });
      // the guided tour starts on the first screen after the wizard
      localStorage.setItem("temisz.tour", "start");
      await refresh();
    });

  return (
    <div className="flex min-h-full items-center justify-center px-6 py-10">
      <div className="w-full max-w-2xl">
        <p className="mb-2 text-3xl font-semibold">{t("onboarding.title")}</p>
        <ol className="mb-6 flex flex-wrap gap-2" aria-label={t("onboarding.title")}>
          {steps.map((s, i) => (
            <li key={s} aria-current={i === step ? "step" : undefined} className={cx("rounded-full border px-3 py-1 text-sm", i === step ? "border-slate bg-lichen font-semibold" : i < step ? "border-seam text-slate" : "border-seam text-granite")}>
              {i + 1}. {s}
            </li>
          ))}
        </ol>
        <div className="flex flex-col gap-4 rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)] px-8 py-7">
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
            {step < 1 ? (
              <Button variant="primary" busy={busy} onClick={next}>
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
