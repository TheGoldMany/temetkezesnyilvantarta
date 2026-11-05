import { useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../lib/api";
import type { BurialResult, EventType } from "../../lib/types";
import { Alert, Button, ErrorAlert, Field, PageHeader, Panel, Select, TextField } from "../../components/ui";
import { useCurrentCemetery, useInvalidate, usePlotTypes } from "../../lib/queries";
import { fmtDate } from "../../lib/format";
import { split } from "./names";

const EMPTY = {
  folio: "",
  eventDate: "",
  eventType: "coffin_burial" as EventType,
  fullName: "",
  birthName: "",
  birthDate: "",
  motherName: "",
  lastResidence: "",
  deathDate: "",
  plotIdentifier: "",
  holderName: "",
  holderAddress: "",
  notes: "",
};

/**
 * FR-IMP-03: fast keyboard entry from the paper register, keeping the old
 * folio. Enter moves to the next field, Ctrl+Enter saves and starts a new row.
 */
export function QuickEntry() {
  const { t } = useTranslation();
  const { current } = useCurrentCemetery();
  const types = usePlotTypes();
  const invalidate = useInvalidate();
  const [v, setV] = useState({ ...EMPTY });
  const [plotType, setPlotType] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [saved, setSaved] = useState<{ folio: number; name: string; date: string; plot: string }[]>([]);
  const [busy, setBusy] = useState(false);
  const formRef = useRef<HTMLFormElement>(null);
  const set = (k: keyof typeof EMPTY) => (x: string) => setV((s) => ({ ...s, [k]: x }));

  const save = async () => {
    if (!current) return;
    setBusy(true);
    setError(null);
    try {
      const plotId = await api<string>("plots.ensure", { cemeteryId: current.id, identifier: v.plotIdentifier.trim().toUpperCase(), plotTypeId: plotType || null });
      const [prefix, family, given] = split(v.fullName);
      const holder = v.holderName.trim()
        ? (() => {
            const [hp, hf, hg] = split(v.holderName);
            return { mode: "new" as const, person: { namePrefix: hp, familyName: hf, givenName: hg, address: v.holderAddress } };
          })()
        : null;
      const r = await api<BurialResult>("burial.record", {
        input: {
          eventType: v.eventType,
          eventDate: v.eventDate || v.deathDate,
          container: v.eventType === "urn_placement" ? "urn" : "coffin",
          deceased: {
            mode: "new",
            deceased: {
              namePrefix: prefix,
              familyName: family,
              givenName: given,
              birthName: v.birthName,
              birthDate: v.birthDate || null,
              motherName: v.motherName,
              lastResidence: v.lastResidence,
              deathDate: v.deathDate || null,
              sex: "",
            },
          },
          plotId,
          cryptSlotId: null,
          arranger: holder,
          holder,
          holderRelation: null,
          lease: { create: false },
          historicalFolio: v.folio ? Number(v.folio) : null,
          overrideReason: null,
          notes: v.notes,
        },
      });
      setSaved((s) => [{ folio: r.folio, name: v.fullName, date: v.eventDate || v.deathDate, plot: v.plotIdentifier.toUpperCase() }, ...s].slice(0, 30));
      // next row: keep date context, increment the paper folio
      setV({ ...EMPTY, eventType: v.eventType, folio: v.folio ? String(Number(v.folio) + 1) : "" });
      await invalidate();
      formRef.current?.querySelector<HTMLInputElement>("input")?.focus();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLFormElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void save();
      return;
    }
    if (e.key === "Enter" && (e.target as HTMLElement).tagName === "INPUT") {
      e.preventDefault();
      const fields = Array.from(formRef.current?.querySelectorAll<HTMLElement>("input,select") ?? []);
      const i = fields.indexOf(e.target as HTMLElement);
      fields[i + 1]?.focus();
    }
  };

  return (
    <>
      <PageHeader title={t("quick.title")} subtitle={current?.name} />
      <div className="mb-4">
        <Alert>{t("quick.intro")}</Alert>
      </div>
      <div className="grid gap-6 xl:grid-cols-[3fr_2fr]">
        <Panel>
          <form ref={formRef} onKeyDown={onKeyDown} onSubmit={(e) => (e.preventDefault(), void save())} className="grid gap-4 md:grid-cols-3" data-testid="quick-entry">
            <TextField label={t("fields.historicalFolio")} type="number" value={v.folio} onChange={set("folio")} inputProps={{ autoFocus: true }} />
            <TextField label={t("fields.eventDate")} type="date" value={v.eventDate} onChange={set("eventDate")} />
            <Field label={t("fields.eventType")}>
              <Select value={v.eventType} onChange={(e) => set("eventType")(e.target.value)}>
                {["coffin_burial", "urn_placement", "reburial"].map((x) => (
                  <option key={x} value={x}>
                    {t(`eventType.${x}`)}
                  </option>
                ))}
              </Select>
            </Field>
            <TextField className="md:col-span-2" label={`${t("fields.familyName")} és ${t("fields.givenName").toLowerCase()}`} value={v.fullName} onChange={set("fullName")} required />
            <TextField label={t("fields.birthName")} value={v.birthName} onChange={set("birthName")} />
            <TextField label={t("fields.birthDate")} type="date" value={v.birthDate} onChange={set("birthDate")} />
            <TextField label={t("fields.motherName")} value={v.motherName} onChange={set("motherName")} />
            <TextField label={t("fields.deathDate")} type="date" value={v.deathDate} onChange={set("deathDate")} required />
            <TextField className="md:col-span-2" label={t("fields.lastResidence")} value={v.lastResidence} onChange={set("lastResidence")} />
            <TextField label={t("fields.plotIdentifier")} value={v.plotIdentifier} onChange={(x) => set("plotIdentifier")(x.toUpperCase())} required hint="A/3/12" />
            <TextField label={t("quick.holderName")} value={v.holderName} onChange={set("holderName")} />
            <TextField className="md:col-span-2" label={t("quick.holderAddress")} value={v.holderAddress} onChange={set("holderAddress")} />
            <Field label={t("io.defaultPlotType")}>
              <Select value={plotType} onChange={(e) => setPlotType(e.target.value)}>
                <option value="">{types.data?.find((x) => x.code === "single")?.name}</option>
                {(types.data ?? []).map((pt) => (
                  <option key={pt.id} value={pt.id}>
                    {pt.name}
                  </option>
                ))}
              </Select>
            </Field>
            <TextField className="md:col-span-2" label={t("common.notes")} value={v.notes} onChange={set("notes")} />
            <div className="md:col-span-3">
              <ErrorAlert error={error} />
            </div>
            <div className="md:col-span-3">
              <Button type="submit" variant="primary" size="lg" busy={busy}>
                {t("quick.save")}
              </Button>
            </div>
          </form>
        </Panel>
        <Panel title={t("quick.recent")}>
          <ul className="flex flex-col gap-2" data-testid="quick-recent">
            {saved.map((s, i) => (
              <li key={i}>
                <strong>{s.folio}.</strong> {s.name}, {fmtDate(s.date)}, {s.plot}
              </li>
            ))}
          </ul>
        </Panel>
      </div>
    </>
  );
}
