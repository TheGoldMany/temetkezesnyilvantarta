import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import type { Deceased, Person, PersonRef, PlotSummary, Page, Verdict, PlotStatus } from "../lib/types";
import { Alert, Checkbox, Field, Input, Plaque, RadioGroup, Select, StatusBadge, TextField, cx } from "./ui";
import { fullName } from "../lib/format";

export const emptyDeceased: Partial<Deceased> = {
  namePrefix: "",
  familyName: "",
  givenName: "",
  birthName: "",
  sex: "",
  birthPlace: "",
  birthDate: "",
  motherName: "",
  lastResidence: "",
  deathPlace: "",
  deathDate: "",
  deathCertNo: "",
  unknownIdentity: false,
  authorityName: "",
  caseNumber: "",
  notes: "",
};

export const emptyPerson: Partial<Person> = { namePrefix: "", familyName: "", givenName: "", address: "", mailAddress: "", phone: "", email: "", notes: "" };

/** FR-ELH-01/05/08: deceased data with Hungarian name order and conditional fields. */
export function DeceasedForm({ value, onChange, errorField }: { value: Partial<Deceased>; onChange: (d: Partial<Deceased>) => void; errorField?: string | null }) {
  const { t } = useTranslation();
  const set = (k: keyof Deceased) => (v: string) => onChange({ ...value, [k]: v });
  const err = (f: string) => (errorField === f ? t("common.required") : null);
  const req = !value.unknownIdentity;
  return (
    <div className="flex flex-col gap-4">
      <Checkbox label={t("fields.unknownIdentity")} checked={!!value.unknownIdentity} onChange={(v) => onChange({ ...value, unknownIdentity: v })} />
      {value.unknownIdentity && (
        <div className="grid gap-4 rounded-md border border-warn/40 bg-warn-bg/50 p-4 md:grid-cols-2">
          <TextField label={t("fields.authorityName")} value={value.authorityName} onChange={set("authorityName")} required error={err("authorityName")} />
          <TextField label={t("fields.caseNumber")} value={value.caseNumber} onChange={set("caseNumber")} required error={err("caseNumber")} />
        </div>
      )}
      <div className="grid gap-4 md:grid-cols-[8rem_1fr_1fr]">
        <TextField label={t("fields.namePrefix")} value={value.namePrefix} onChange={set("namePrefix")} hint={t("fields.namePrefixHint")} />
        <TextField label={t("fields.familyName")} value={value.familyName} onChange={set("familyName")} required={req} error={err("familyName")} inputProps={{ autoFocus: true }} />
        <TextField label={t("fields.givenName")} value={value.givenName} onChange={set("givenName")} required={req} error={err("givenName")} />
      </div>
      <div className="grid gap-4 md:grid-cols-[1fr_12rem]">
        <TextField label={t("fields.birthName")} value={value.birthName} onChange={set("birthName")} />
        <Field label={t("fields.sex")}>
          <Select value={value.sex ?? ""} onChange={(e) => set("sex")(e.target.value)}>
            {(["", "male", "female", "unknown"] as const).map((s) => (
              <option key={s} value={s}>
                {t(`sex.${s}`)}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <TextField label={t("fields.birthPlace")} value={value.birthPlace} onChange={set("birthPlace")} />
        <TextField label={t("fields.birthDate")} type="date" value={value.birthDate} onChange={set("birthDate")} required={req} error={err("birthDate")} />
      </div>
      <TextField label={t("fields.motherName")} value={value.motherName} onChange={set("motherName")} required={req} error={err("motherName")} />
      <TextField label={t("fields.lastResidence")} value={value.lastResidence} onChange={set("lastResidence")} required={req} error={err("lastResidence")} />
      <div className="grid gap-4 md:grid-cols-3">
        <TextField label={t("fields.deathPlace")} value={value.deathPlace} onChange={set("deathPlace")} />
        <TextField label={t("fields.deathDate")} type="date" value={value.deathDate} onChange={set("deathDate")} required error={err("deathDate")} />
        <TextField label={t("fields.deathCertNo")} value={value.deathCertNo} onChange={set("deathCertNo")} />
      </div>
    </div>
  );
}

export function PersonFields({ value, onChange, errorField }: { value: Partial<Person>; onChange: (p: Partial<Person>) => void; errorField?: string | null }) {
  const { t } = useTranslation();
  const set = (k: keyof Person) => (v: string) => onChange({ ...value, [k]: v });
  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-4 md:grid-cols-[8rem_1fr_1fr]">
        <TextField label={t("fields.namePrefix")} value={value.namePrefix} onChange={set("namePrefix")} />
        <TextField label={t("fields.familyName")} value={value.familyName} onChange={set("familyName")} required error={errorField === "familyName" ? t("common.required") : null} />
        <TextField label={t("fields.givenName")} value={value.givenName} onChange={set("givenName")} />
      </div>
      <TextField label={t("fields.address")} value={value.address} onChange={set("address")} />
      <div className="grid gap-4 md:grid-cols-3">
        <TextField label={t("fields.mailAddress")} value={value.mailAddress} onChange={set("mailAddress")} />
        <TextField label={t("fields.phone")} value={value.phone} onChange={set("phone")} type="tel" />
        <TextField label={t("fields.email")} value={value.email} onChange={set("email")} type="email" />
      </div>
    </div>
  );
}

/** Choose an existing person or enter a new one (FR-UGY-01/02). */
export function PersonPicker({ label, value, onChange }: { label: string; value: PersonRef | null; onChange: (r: PersonRef | null) => void }) {
  const { t } = useTranslation();
  const mode = value?.mode ?? "new";
  const [q, setQ] = useState("");
  const { data = [] } = useQuery({
    queryKey: ["persons", q],
    queryFn: () => api<Person[]>("persons.list", { query: q, limit: 8 }),
    enabled: mode === "existing" && q.trim().length >= 2,
  });
  useEffect(() => {
    if (!value) onChange({ mode: "new", person: { ...emptyPerson } });
  }, [value, onChange]);
  return (
    <fieldset className="flex flex-col gap-3 rounded-lg border border-seam px-5 py-4">
      <legend className="px-1 text-lg font-semibold">{label}</legend>
      <div className="flex gap-2" role="radiogroup">
        {(["new", "existing"] as const).map((m) => (
          <button
            key={m}
            type="button"
            role="radio"
            aria-checked={mode === m}
            onClick={() => onChange(m === "new" ? { mode: "new", person: { ...emptyPerson } } : { mode: "existing", id: "" })}
            className={cx("min-h-10 rounded-md border px-3 font-semibold", mode === m ? "border-moss bg-lichen" : "border-seam bg-slab text-granite")}
          >
            {m === "new" ? t("wizard.personNew") : t("wizard.personExisting")}
          </button>
        ))}
      </div>
      {value?.mode === "new" && <PersonFields value={value.person} onChange={(p) => onChange({ mode: "new", person: p })} />}
      {value?.mode === "existing" && (
        <div className="flex flex-col gap-2">
          <TextField label={t("wizard.personSearch")} value={q} onChange={setQ} />
          <ul className="flex flex-col gap-1">
            {data.map((p) => (
              <li key={p.id}>
                <label className={cx("flex cursor-pointer items-center gap-3 rounded-md border px-3 py-2", value.id === p.id ? "border-moss bg-lichen/60" : "border-seam")}>
                  <input type="radio" className="size-5 accent-[var(--c-moss)]" checked={value.id === p.id} onChange={() => onChange({ mode: "existing", id: p.id })} />
                  <span>
                    <span className="font-semibold">{fullName(p)}</span>
                    <span className="block text-sm text-granite">{p.masked ? t("common.hidden") : p.address}</span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        </div>
      )}
    </fieldset>
  );
}

/** Picks a plot by identifier with status shown; optionally restricted to some statuses. */
export function PlotPicker({ cemeteryId, value, onChange, allowed }: { cemeteryId: string | null; value: string | null; onChange: (id: string | null, p?: PlotSummary) => void; allowed?: PlotStatus[] }) {
  const { t } = useTranslation();
  const [q, setQ] = useState("");
  const id = useId();
  const { data } = useQuery({
    queryKey: ["plotPicker", cemeteryId, q, allowed],
    queryFn: () => api<Page<PlotSummary>>("plots.list", { filter: { cemeteryId, identifierPrefix: q || null, statuses: allowed ?? [], limit: 12 } }),
    enabled: !!cemeteryId,
  });
  return (
    <div className="flex flex-col gap-2">
      <Field label={t("wizard.pickPlot")} hint={t("wizard.pickPlotHint")} htmlFor={id}>
        <Input id={id} value={q} onChange={(e) => setQ(e.target.value.toUpperCase())} placeholder="A/3/12" />
      </Field>
      <ul className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3" aria-label={t("wizard.pickPlot")}>
        {(data?.items ?? []).map((p) => (
          <li key={p.id}>
            <label className={cx("flex cursor-pointer items-center justify-between gap-3 rounded-md border px-3 py-2", value === p.id ? "border-moss bg-lichen/60" : "border-seam bg-slab")}>
              <span className="flex items-center gap-3">
                <input type="radio" name={id} className="size-5 accent-[var(--c-moss)]" checked={value === p.id} onChange={() => onChange(p.id, p)} />
                <Plaque identifier={p.identifier} />
              </span>
              <span className="text-right text-sm">
                <StatusBadge status={p.status} protectedPlot={p.protected} />
                <span className="block text-granite">{p.typeName}</span>
              </span>
            </label>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function VerdictView({ verdict }: { verdict: Verdict | null | undefined }) {
  const { t } = useTranslation();
  if (!verdict) return null;
  const line = (r: { code: string; params: Record<string, string> }) => t(`burialCheck.${r.code}`, { ...r.params, status: r.params.status ? t(`status.${r.params.status}`) : undefined });
  return (
    <div className="flex flex-col gap-2" data-testid="verdict">
      {verdict.allowed ? (
        <Alert tone="ok" title={t("burialCheck.allowed")} />
      ) : (
        <Alert tone="error" title={t("burialCheck.notAllowed")}>
          <ul className="list-disc pl-5">
            {verdict.blockers.map((b, i) => (
              <li key={i}>{line(b)}</li>
            ))}
          </ul>
        </Alert>
      )}
      {verdict.warnings.length > 0 && (
        <Alert tone="warn">
          <ul className="list-disc pl-5">
            {verdict.warnings.map((w, i) => (
              <li key={i}>{line(w)}</li>
            ))}
          </ul>
        </Alert>
      )}
    </div>
  );
}

export function ReasonField({ value, onChange, label }: { value: string; onChange: (v: string) => void; label?: string }) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <Field label={label ?? t("common.reason")} hint={t("common.reasonHint")} required htmlFor={id}>
      <textarea id={id} value={value} onChange={(e) => onChange(e.target.value)} className="min-h-20 w-full rounded-md border border-seam bg-slab px-3 py-2 focus:border-moss focus:outline-none" />
    </Field>
  );
}

export { RadioGroup };
