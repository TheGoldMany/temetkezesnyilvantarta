import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { CheckCircle2 } from "lucide-react";
import { useApp } from "../../app/context";
import { api, ApiError } from "../../lib/api";
import type { BurialInput, BurialPreview, BurialResult, Deceased, EventType, PersonRef, PlotDetail } from "../../lib/types";
import { Alert, Button, Checkbox, DefinitionList, ErrorAlert, Field, Input, PageHeader, Plaque, Select, TextField, cx } from "../../components/ui";
import { DeceasedForm, PersonPicker, PlotPicker, ReasonField, VerdictView, emptyDeceased } from "../../components/forms";
import { fmtDate, fmtHuf, fullName, todayIso } from "../../lib/format";
import { useCurrentCemetery, useInvalidate } from "../../lib/queries";
import { PrintButton } from "../prints/PrintButton";
import { BurialPack } from "../prints/BurialPack";

const STEPS = ["deceased", "place", "people", "lease", "review"] as const;
type Step = (typeof STEPS)[number];

/** UC-01 (new burial) and UC-02 (reburial, when opened from a plot). */
export function NewBurial({ plotId: presetPlot, eventType: presetType, bookingId }: { plotId?: string; eventType?: string; bookingId?: string }) {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const { current } = useCurrentCemetery();
  const invalidate = useInvalidate();
  const isReburial = presetType === "reburial";
  const [step, setStep] = useState<Step>("deceased");
  const [deceased, setDeceased] = useState<Partial<Deceased>>({ ...emptyDeceased });
  const [existingDeceasedId, setExistingDeceasedId] = useState<string | null>(null);
  const [ignoreSimilar, setIgnoreSimilar] = useState(false);
  const [eventType, setEventType] = useState<EventType>(isReburial ? "reburial" : "coffin_burial");
  const [container, setContainer] = useState<"coffin" | "urn">("coffin");
  const [eventDate, setEventDate] = useState(todayIso());
  const [plotId, setPlotId] = useState<string | null>(presetPlot ?? null);
  const [slotId, setSlotId] = useState<string>("");
  const [arranger, setArranger] = useState<PersonRef | null>(null);
  const [holderSame, setHolderSame] = useState(true);
  const [holder, setHolder] = useState<PersonRef | null>(null);
  const [relation, setRelation] = useState("");
  const [leaseCreate, setLeaseCreate] = useState(true);
  const [leaseYears, setLeaseYears] = useState<string>("");
  const [leaseFee, setLeaseFee] = useState<string>("");
  const [notes, setNotes] = useState("");
  // no plot: urn taken home, or ashes scattered somewhere else
  const [offSite, setOffSite] = useState<"" | "home" | "elsewhere">("");
  const [receiverName, setReceiverName] = useState("");
  const [receiverAddress, setReceiverAddress] = useState("");
  const [destination, setDestination] = useState("");
  const [override, setOverride] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<BurialResult | null>(null);

  // FR-SZE-02: a funeral booked in the calendar fills the form
  const booking = useQuery({
    queryKey: ["booking", bookingId],
    queryFn: () => api<{ startsAt: string; plotId: string | null; deceasedName: string; container: "" | "coffin" | "urn" }>("bookings.get", { id: bookingId }),
    enabled: !!bookingId,
  });
  useEffect(() => {
    const b = booking.data;
    if (!b) return;
    setEventDate(b.startsAt.slice(0, 10));
    if (b.plotId) setPlotId(b.plotId);
    if (b.container === "urn") setEventType("urn_placement");
    if (b.container === "coffin") setEventType("coffin_burial");
    const parts = b.deceasedName.trim().split(/\s+/).filter(Boolean);
    if (parts.length) setDeceased((d) => ({ ...d, familyName: parts[0], givenName: parts.slice(1).join(" ") }));
  }, [booking.data]);

  const effectiveContainer = eventType === "urn_placement" ? "urn" : eventType === "coffin_burial" ? "coffin" : eventType === "ash_scattering" ? "none" : container;

  const plot = useQuery({ queryKey: ["plot", plotId], queryFn: () => api<PlotDetail>("plots.get", { id: plotId }), enabled: !!plotId });
  const preview = useQuery({
    queryKey: ["preview", plotId, eventDate, effectiveContainer],
    queryFn: () => api<BurialPreview>("burial.preview", { plotId, eventDate, container: effectiveContainer }),
    enabled: !!plotId && /^\d{4}-\d{2}-\d{2}$/.test(eventDate),
  });
  const similar = useQuery({
    queryKey: ["similar", deceased.familyName, deceased.givenName],
    queryFn: () => api<Deceased[]>("deceased.similar", { familyName: deceased.familyName, givenName: deceased.givenName, birthDate: deceased.birthDate || null }),
    enabled: (deceased.familyName ?? "").trim().length >= 2 && (deceased.givenName ?? "").trim().length >= 2 && !existingDeceasedId,
  });

  useEffect(() => {
    const p = preview.data?.lease;
    if (p) {
      setLeaseYears(String(p.durationYears));
      setLeaseFee(String(p.feeHuf));
    }
  }, [preview.data?.lease]);

  const hasHolder = preview.data?.hasPrimaryHolder ?? false;
  const holderName = plot.data?.holders.find((h) => h.role === "primary" && !h.endDate)?.personName;
  const category = plot.data?.plotType.category;
  const needsHolder = !hasHolder && category !== "scatter" && category !== "common";
  const isCrypt = category === "crypt" && effectiveContainer === "coffin";

  const input: BurialInput = useMemo(
    () => ({
      eventType,
      eventDate,
      container: effectiveContainer,
      deceased: existingDeceasedId ? { mode: "existing", id: existingDeceasedId } : { mode: "new", deceased: { ...deceased, birthDate: deceased.birthDate || null, deathDate: deceased.deathDate || null } },
      plotId: offSite ? null : plotId,
      cryptSlotId: slotId || null,
      arranger: offSite && arranger && arranger.mode === "new" && !arranger.person.familyName ? null : arranger,
      cemeteryId: current?.id ?? null,
      receiverName,
      receiverAddress,
      destination,
      holder: needsHolder && !holderSame ? holder : null,
      holderRelation: relation || null,
      lease: { create: leaseCreate && !!preview.data?.lease, durationYears: leaseYears ? Number(leaseYears) : null, feeHuf: leaseFee !== "" ? Number(leaseFee) : null },
      historicalFolio: null,
      overrideReason: override.trim() || null,
      notes,
    }),
    [eventType, eventDate, effectiveContainer, existingDeceasedId, deceased, plotId, slotId, arranger, needsHolder, holderSame, holder, relation, leaseCreate, preview.data?.lease, leaseYears, leaseFee, override, notes, offSite, current?.id, receiverName, receiverAddress, destination],
  );

  const errorField = error instanceof ApiError && error.code === "validation" ? error.params.field : null;
  const stepIdx = STEPS.indexOf(step);
  const blocked = preview.data && !preview.data.verdict.allowed;

  const canNext = (): boolean => {
    if (step === "deceased") {
      if (existingDeceasedId) return true;
      // the required data first: otherwise the last step sends the user back here
      const d = deceased;
      const filled = (v?: string | null) => !!v && v.trim().length > 0;
      const complete = d.unknownIdentity
        ? filled(d.authorityName) && filled(d.caseNumber) && filled(d.deathDate)
        : filled(d.familyName) && filled(d.givenName) && filled(d.birthDate) && filled(d.motherName) && filled(d.lastResidence) && filled(d.deathDate);
      return complete && (ignoreSimilar || (similar.data?.length ?? 0) === 0 || !similar.isFetched);
    }
    if (step === "place" && offSite === "home") return !!eventDate && receiverName.trim().length > 1;
    if (step === "place" && offSite === "elsewhere") return !!eventDate && destination.trim().length > 2;
    if (step === "place") return !!plotId && !!eventDate && (!blocked || can("override_checks"));
    if (step === "people" && offSite) return true;
    if (step === "people") return !!arranger && (arranger.mode === "existing" ? !!arranger.id : !!arranger.person.familyName) && (!needsHolder || holderSame || !!holder);
    return true;
  };

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api<BurialResult>("burial.record", { input });
      if (bookingId) await api("bookings.status", { id: bookingId, status: "done", eventId: r.event.id }).catch(() => undefined);
      setResult(r);
      await invalidate();
    } catch (e) {
      setError(e);
      if (e instanceof ApiError && e.code === "validation") {
        const f = e.params.field;
        if (["familyName", "givenName", "motherName", "lastResidence", "birthDate", "deathDate", "authorityName", "caseNumber"].includes(f)) setStep("deceased");
      }
    } finally {
      setBusy(false);
    }
  };

  if (result) {
    return (
      <div className="max-w-3xl">
        <PageHeader title={t(isReburial ? "plots.reburial" : "wizard.title")} />
        <div className="flex flex-col gap-5 rounded-lg border border-moss/40 bg-slab px-7 py-6" data-testid="burial-done">
          <p className="flex items-center gap-3 text-xl font-semibold">
            <CheckCircle2 className="size-7 text-accent-ink" aria-hidden />
            {t("wizard.done", { folio: result.folio })}
          </p>
          <DefinitionList
            items={[
              [t("fields.folio"), <strong key="f">{result.folio}.</strong>],
              [t("fields.eventType"), t(`eventType.${result.event.eventType}`)],
              [t("fields.eventDate"), fmtDate(result.event.eventDate)],
              [t("fields.plot"), <Plaque key="p" identifier={plot.data?.plot.identifier} />],
              ...(result.lease ? ([[t(`leaseKind.${result.lease.kind}`), `${fmtDate(result.lease.startDate)} – ${fmtDate(result.lease.expiryDate)}, ${fmtHuf(result.lease.feeHuf)}`]] as [string, string][]) : []),
            ]}
          />
          {can("print") ? (
            <BurialPack eventId={result.event.id} />
          ) : (
            offSite === "home" && <PrintButton doc="urn_receipt" args={{ eventId: result.event.id }} label={t("removal.printReceipt")} />
          )}
          <div className="flex flex-wrap gap-3">
            {!offSite && <Button onClick={() => plotId && nav.go({ view: "plot", id: plotId })}>{t("wizard.openPlot")}</Button>}
            <Button variant="quiet" onClick={() => nav.go({ view: "newBurial" })}>
              {t("wizard.another")}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-4xl">
      <PageHeader title={isReburial ? t("plots.reburial") : t("wizard.title")} subtitle={plot.data && <Plaque identifier={plot.data.plot.identifier} />} />
      <ol className="mb-6 flex flex-wrap gap-2">
        {STEPS.map((s, i) => (
          <li key={s}>
            <button
              type="button"
              onClick={() => i < stepIdx && setStep(s)}
              aria-current={s === step ? "step" : undefined}
              className={cx("rounded-md border px-3 py-1 text-sm", s === step ? "border-slate bg-lichen font-semibold" : i < stepIdx ? "border-seam text-slate hover:border-granite" : "border-seam text-granite")}
            >
              {i + 1}. {t(`wizard.steps.${s}`)}
            </button>
          </li>
        ))}
      </ol>

      <div className="flex flex-col gap-5 rounded-xl border border-seam bg-slab px-7 py-6">
        {step === "deceased" && (
          <>
            {existingDeceasedId ? (
              <Alert tone="info" title={fullName(similar.data?.find((d) => d.id === existingDeceasedId) ?? {})}>
                <Button variant="quiet" onClick={() => setExistingDeceasedId(null)}>
                  {t("wizard.similarIgnore")}
                </Button>
              </Alert>
            ) : (
              <DeceasedForm value={deceased} onChange={setDeceased} errorField={errorField} />
            )}
            {!existingDeceasedId && !ignoreSimilar && (similar.data?.length ?? 0) > 0 && (
              <Alert tone="warn" title={t("wizard.similarFound")}>
                <ul className="mt-2 flex flex-col gap-2">
                  {similar.data!.map((d) => (
                    <li key={d.id} className="flex flex-wrap items-center gap-3">
                      <span>
                        <strong>{fullName(d)}</strong>, {fmtDate(d.birthDate)} – {fmtDate(d.deathDate)}, {d.motherName}
                      </span>
                      <Button onClick={() => setExistingDeceasedId(d.id)}>{t("wizard.similarUse")}</Button>
                    </li>
                  ))}
                </ul>
                <Button variant="quiet" className="mt-2" onClick={() => setIgnoreSimilar(true)}>
                  {t("wizard.similarIgnore")}
                </Button>
              </Alert>
            )}
          </>
        )}

        {step === "place" && (
          <>
            {!isReburial && (
              <Field label={t("wizard.eventKind")}>
                <Select
                  value={offSite ? `off_${offSite}` : eventType}
                  name="eventKind"
                  onChange={(e) => {
                    const v = e.target.value;
                    if (v === "off_home") {
                      setOffSite("home");
                      setEventType("urn_release");
                    } else if (v === "off_elsewhere") {
                      setOffSite("elsewhere");
                      setEventType("ash_scattering");
                    } else {
                      setOffSite("");
                      setEventType(v as EventType);
                    }
                  }}
                >
                  {(["coffin_burial", "urn_placement", "ash_scattering", "common_grave"] as EventType[]).map((et) => (
                    <option key={et} value={et}>
                      {t(`eventType.${et}`)}
                    </option>
                  ))}
                  <option value="off_home">{t("wizard.offHome")}</option>
                  <option value="off_elsewhere">{t("wizard.offElsewhere")}</option>
                </Select>
              </Field>
            )}
            {(isReburial || eventType === "common_grave") && (
              <Field label={t("fields.eventType")}>
                <Select value={container} onChange={(e) => setContainer(e.target.value as "coffin" | "urn")}>
                  <option value="coffin">{t("container.coffin")}</option>
                  <option value="urn">{t("container.urn")}</option>
                </Select>
              </Field>
            )}
            <TextField label={t("fields.eventDate")} type="date" value={eventDate} onChange={setEventDate} required />
            {offSite === "home" && (
              <>
                <Alert>{t("wizard.offHomeHint")}</Alert>
                <TextField label={t("fields.receiverName")} value={receiverName} onChange={setReceiverName} required inputProps={{ name: "receiverName" }} />
                <TextField label={t("fields.receiverAddress")} value={receiverAddress} onChange={setReceiverAddress} inputProps={{ name: "receiverAddress" }} />
                <TextField label={t("wizard.offHomeWhere")} value={destination} onChange={setDestination} hint={t("wizard.offHomeWhereHint")} />
              </>
            )}
            {offSite === "elsewhere" && (
              <>
                <Alert>{t("wizard.offElsewhereHint")}</Alert>
                <TextField label={t("wizard.scatterPlace")} value={destination} onChange={setDestination} required hint={t("wizard.destinationHint")} inputProps={{ name: "destination" }} />
              </>
            )}
            {!presetPlot && !offSite && (
              <>
                <p className="text-granite">{t("wizard.onlyFreeShown")}</p>
                <PlotPicker cemeteryId={current?.id ?? null} value={plotId} onChange={(id) => setPlotId(id)} allowed={eventType === "ash_scattering" || eventType === "common_grave" ? undefined : ["free", "reserved"]} />
              </>
            )}
            {isCrypt && preview.data && (
              <Field label={t("wizard.cryptSlot")}>
                <Select value={slotId} onChange={(e) => setSlotId(e.target.value)}>
                  <option value="">{t("wizard.autoSlot")}</option>
                  {preview.data.cryptFreeSlots.map((s) => (
                    <option key={s.id} value={s.id}>
                      {t("plots.slot", { no: s.slotNo })}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            {!offSite && <VerdictView verdict={preview.data?.verdict} />}
            {blocked && can("override_checks") && (
              <div className="rounded-md border border-alert/40 p-4">
                <p className="font-semibold">{t("burialCheck.overrideTitle")}</p>
                <p className="mb-3 text-sm text-granite">{t("burialCheck.overrideHint")}</p>
                <ReasonField value={override} onChange={setOverride} />
              </div>
            )}
          </>
        )}

        {step === "people" && (
          <>
            {offSite && <Alert>{t("wizard.offPeopleHint")}</Alert>}
            <PersonPicker label={t("wizard.arranger")} value={arranger} onChange={setArranger} />
            {offSite ? null : hasHolder ? (
              <Alert>{t("wizard.holderExisting", { name: holderName ?? "" })}</Alert>
            ) : (
              needsHolder && (
                <>
                  <Checkbox label={t("wizard.holderSame")} checked={holderSame} onChange={setHolderSame} />
                  {!holderSame && <PersonPicker label={t("fields.holder")} value={holder} onChange={setHolder} />}
                </>
              )
            )}
            <TextField label={t("fields.relation")} value={relation} onChange={setRelation} hint="pl. fia, lánya, házastársa" />
          </>
        )}

        {step === "lease" && offSite && <Alert>{t("wizard.offNoLease")}</Alert>}
        {step === "lease" && !offSite && (
          <>
            {preview.data?.lease ? (
              <>
                <Alert tone="info">
                  {t("wizard.leaseProposal", {
                    kind: t(`leaseKind.${preview.data.lease.kind}`),
                    start: fmtDate(preview.data.lease.startDate),
                    expiry: fmtDate(preview.data.lease.expiryDate),
                    years: preview.data.lease.durationYears,
                  })}
                  {preview.data.lease.extensionDays > 0 && <span className="block">{t("wizard.extension", { days: preview.data.lease.extensionDays })}</span>}
                </Alert>
                <Checkbox label={t("wizard.leaseCreate")} checked={leaseCreate} onChange={setLeaseCreate} />
                {leaseCreate && (
                  <div className="grid gap-4 md:grid-cols-2">
                    <TextField label={t("wizard.duration")} type="number" value={leaseYears} onChange={setLeaseYears} inputProps={{ min: 1, max: 200 }} />
                    {preview.data.lease.feeExempt ? (
                      <Field label={t("wizard.fee")}>
                        <p className="py-2 font-semibold">{t("wizard.feeExempt")}</p>
                      </Field>
                    ) : (
                      <TextField label={`${t("wizard.fee")} (Ft)`} type="number" value={leaseFee} onChange={setLeaseFee} hint={!preview.data.lease.feeRateId ? t("lease.noRate") : fmtHuf(Number(leaseFee || 0))} inputProps={{ min: 0 }} />
                    )}
                  </div>
                )}
              </>
            ) : (
              <Alert>{t("wizard.leaseNone")}</Alert>
            )}
            <Field label={t("common.notes")}>
              <Input value={notes} onChange={(e) => setNotes(e.target.value)} />
            </Field>
          </>
        )}

        {step === "review" && (
          <>
            <h2 className="text-xl">{t("wizard.reviewTitle")}</h2>
            <DefinitionList
              items={[
                [t("fields.familyName"), existingDeceasedId ? fullName(similar.data?.find((d) => d.id === existingDeceasedId) ?? {}) : fullName(deceased) || t("fields.unknownIdentity")],
                [t("fields.birthDate"), fmtDate(deceased.birthDate || null)],
                [t("fields.deathDate"), fmtDate(deceased.deathDate || null)],
                [t("fields.motherName"), deceased.motherName],
                [t("fields.eventType"), offSite ? t(offSite === "home" ? "wizard.offHome" : "wizard.offElsewhere") : t(`eventType.${eventType}`)],
                [t("fields.eventDate"), fmtDate(eventDate)],
                ...(offSite === "home"
                  ? ([[t("fields.receiverName"), `${receiverName}${receiverAddress ? `, ${receiverAddress}` : ""}`]] as [string, string][])
                  : offSite === "elsewhere"
                    ? ([[t("wizard.scatterPlace"), destination]] as [string, string][])
                    : ([[t("fields.plot"), <Plaque key="p" identifier={plot.data?.plot.identifier} />]] as [string, React.ReactNode][])),
                [t("fields.arranger"), arranger?.mode === "new" ? fullName(arranger.person) || "–" : arranger ? "✓" : "–"],
                ...(preview.data?.lease && leaseCreate ? ([[t(`leaseKind.${preview.data.lease.kind}`), `${leaseYears} év, ${fmtHuf(Number(leaseFee || 0))}`]] as [string, string][]) : []),
              ]}
            />
            {!offSite && <VerdictView verdict={preview.data?.verdict} />}
            <Alert>{t("wizard.reviewFolio")}</Alert>
          </>
        )}
        <ErrorAlert error={error} />
        <div className="flex justify-between gap-3 border-t border-seam pt-5">
          <Button variant="quiet" disabled={stepIdx === 0} onClick={() => setStep(STEPS[stepIdx - 1])}>
            {t("common.previous")}
          </Button>
          {step === "review" ? (
            <Button variant="primary" size="lg" busy={busy} onClick={submit} disabled={!!blocked && !override.trim()}>
              {t("wizard.submit")}
            </Button>
          ) : (
            <Button variant="primary" size="lg" onClick={() => setStep(STEPS[stepIdx + 1])} disabled={!canNext()}>
              {t("common.next")}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
