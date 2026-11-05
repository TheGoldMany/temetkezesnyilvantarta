import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api, ApiError } from "../../lib/api";
import type { InterEvent, Lease, LeaseProposal, PersonRef, Plot, PlotDetail } from "../../lib/types";
import { Alert, Button, Checkbox, Dialog, ErrorAlert, Field, Input, RadioGroup, Select, TextField } from "../../components/ui";
import { PersonPicker, ReasonField } from "../../components/forms";
import { fmtDate, fmtHuf, todayIso } from "../../lib/format";
import { useInvalidate, usePlotTypes } from "../../lib/queries";
import { useToast } from "../../app/toast";
import { PrintButton } from "../prints/PrintButton";

function useSubmit(onDone: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  const toast = useToast();
  const { t } = useTranslation();
  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
      await invalidate();
      toast(t("common.savedToast"));
      onDone();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, run };
}

function Footer({ onClose, onSubmit, busy, label, disabled, danger }: { onClose: () => void; onSubmit: () => void; busy: boolean; label: ReactNode; disabled?: boolean; danger?: boolean }) {
  const { t } = useTranslation();
  return (
    <>
      <Button onClick={onClose}>{t("common.cancel")}</Button>
      <Button variant={danger ? "danger" : "primary"} busy={busy} disabled={disabled} onClick={onSubmit}>
        {label}
      </Button>
    </>
  );
}

/** FR-MEG-01..03: purchase, pre-purchase, renewal. */
export function LeaseDialog({ detail, kind, onClose }: { detail: PlotDetail; kind: "purchase" | "prepurchase" | "renewal"; onClose: () => void }) {
  const { t } = useTranslation();
  const [start, setStart] = useState("");
  const proposal = useQuery({
    queryKey: ["leaseProposal", detail.plot.id, kind, start],
    queryFn: () => api<LeaseProposal>("leases.propose", { plotId: detail.plot.id, kind, start: start || null }),
  });
  const [years, setYears] = useState("");
  const [fee, setFee] = useState("");
  const [person, setPerson] = useState<PersonRef | null>(kind === "renewal" ? null : { mode: "new", person: {} });
  const [created, setCreated] = useState<Lease | null>(null);
  const { busy, error, run } = useSubmit(() => {});
  useEffect(() => {
    if (proposal.data) {
      if (!start) setStart(proposal.data.startDate);
      setYears(String(proposal.data.durationYears));
      setFee(String(proposal.data.feeHuf));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [proposal.data]);
  const title = kind === "purchase" ? t("lease.titlePurchase") : kind === "prepurchase" ? t("lease.titlePrepurchase") : t("lease.titleRenewal");
  if (created) {
    return (
      <Dialog open onClose={onClose} title={title} footer={<Button variant="primary" onClick={onClose}>{t("common.close")}</Button>}>
        <div className="flex flex-col gap-4">
          <Alert tone="ok">
            {t(`leaseKind.${created.kind}`)}: {fmtDate(created.startDate)} – {fmtDate(created.expiryDate)} · {fmtHuf(created.feeHuf)}
          </Alert>
          <div className="flex flex-wrap gap-2">
            <PrintButton doc="lease_certificate" args={{ leaseId: created.id }} label={t("wizard.printCertificate")} />
            {created.feeHuf > 0 && <PrintButton doc="fee_request" args={{ leaseId: created.id }} label={t("wizard.printFeeRequest")} />}
          </div>
        </div>
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={title}
      footer={
        <Footer
          onClose={onClose}
          busy={busy}
          label={kind === "renewal" ? t("lease.createRenewal") : t("lease.create")}
          onSubmit={() =>
            run(async () => {
              const l = await api<Lease>("leases.create", {
                input: { plotId: detail.plot.id, kind, startDate: start, durationYears: Number(years), feeHuf: Number(fee), person, notes: "" },
              });
              setCreated(l);
            })
          }
        />
      }
    >
      <div className="flex flex-col gap-4">
        <div className="grid gap-4 md:grid-cols-3">
          <TextField label={t("lease.start")} type="date" value={start} onChange={setStart} />
          <TextField label={t("lease.duration")} type="number" value={years} onChange={setYears} />
          {proposal.data?.feeExempt ? (
            <Field label={t("lease.fee")}>
              <p className="py-2 font-semibold">{t("wizard.feeExempt")}</p>
            </Field>
          ) : (
            <TextField label={`${t("lease.fee")} (Ft)`} type="number" value={fee} onChange={setFee} hint={!proposal.data?.feeRateId ? t("lease.noRate") : fmtHuf(Number(fee || 0))} />
          )}
        </div>
        {kind !== "renewal" && <PersonPicker label={t("lease.buyer")} value={person} onChange={setPerson} />}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

/** FR-UGY-03: holder change (inheritance, transfer) or adding a contact. */
export function HolderDialog({ plotId, role, onClose }: { plotId: string; role: "primary" | "contact"; onClose: () => void }) {
  const { t } = useTranslation();
  const [person, setPerson] = useState<PersonRef | null>({ mode: "new", person: {} });
  const [date, setDate] = useState(todayIso());
  const bases = t("holder.legalBasisOptions").split("|");
  const [basis, setBasis] = useState(role === "primary" ? bases[0] : "");
  const [doc, setDoc] = useState("");
  const [relation, setRelation] = useState("");
  const { busy, error, run } = useSubmit(onClose);
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={role === "primary" ? t("holder.title") : t("holder.contactTitle")}
      footer={<Footer onClose={onClose} busy={busy} label={t("holder.submit")} onSubmit={() => run(() => api("holders.change", { input: { plotId, person, role, relation, changeDate: date, legalBasis: basis, documentRef: doc } }))} />}
    >
      <div className="flex flex-col gap-4">
        {role === "primary" && <p className="text-granite">{t("holder.intro")}</p>}
        <PersonPicker label={t("fields.holder")} value={person} onChange={setPerson} />
        <div className="grid gap-4 md:grid-cols-3">
          <TextField label={t("holder.changeDate")} type="date" value={date} onChange={setDate} />
          {role === "primary" && (
            <Field label={t("holder.legalBasis")} required>
              <Select value={basis} onChange={(e) => setBasis(e.target.value)}>
                {bases.map((b) => (
                  <option key={b}>{b}</option>
                ))}
              </Select>
            </Field>
          )}
          <TextField label={t("fields.relation")} value={relation} onChange={setRelation} />
        </div>
        {role === "primary" && <TextField label={t("holder.document")} value={doc} onChange={setDoc} />}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

export function RestrictionDialog({ plotId, onClose }: { plotId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const [kind, setKind] = useState<"ban" | "condition">("ban");
  const [text, setText] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const { busy, error, run } = useSubmit(onClose);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("plots.addRestriction")}
      footer={<Footer onClose={onClose} busy={busy} disabled={!text.trim()} label={t("common.add")} onSubmit={() => run(() => api("restrictions.add", { restriction: { plotId, kind, text, validFrom: from || null, validTo: to || null } }))} />}
    >
      <div className="flex flex-col gap-4">
        <RadioGroup
          label={t("plots.restrictionKind")}
          value={kind}
          onChange={setKind}
          options={[
            { value: "ban", label: t("plots.ban") },
            { value: "condition", label: t("plots.condition") },
          ]}
        />
        <TextField label={t("plots.monumentDescription")} value={text} onChange={setText} required hint="pl. talajvíz 2 m-en belül" />
        <div className="grid grid-cols-2 gap-4">
          <TextField label={t("settings.validFrom")} type="date" value={from} onChange={setFrom} />
          <TextField label={t("settings.validTo")} type="date" value={to} onChange={setTo} />
        </div>
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

export function MonumentDialog({ plotId, onClose }: { plotId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const [m, setM] = useState({ entryDate: todayIso(), description: "", contractor: "", material: "", size: "", permitDate: "" });
  const set = (k: keyof typeof m) => (v: string) => setM((s) => ({ ...s, [k]: v }));
  const { busy, error, run } = useSubmit(onClose);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("plots.addMonument")}
      footer={<Footer onClose={onClose} busy={busy} disabled={!m.description.trim()} label={t("common.add")} onSubmit={() => run(() => api("monuments.add", { monument: { ...m, plotId, permitDate: m.permitDate || null, protected: false } }))} />}
    >
      <div className="flex flex-col gap-4">
        <TextField label={t("common.date")} type="date" value={m.entryDate} onChange={set("entryDate")} />
        <TextField label={t("plots.monumentDescription")} value={m.description} onChange={set("description")} required hint="pl. fekete gránit síremlék állítása" />
        <div className="grid gap-4 md:grid-cols-3">
          <TextField label={t("plots.contractor")} value={m.contractor} onChange={set("contractor")} />
          <TextField label={t("plots.material")} value={m.material} onChange={set("material")} />
          <TextField label={t("plots.size")} value={m.size} onChange={set("size")} />
        </div>
        <TextField label={t("plots.permitDate")} type="date" value={m.permitDate} onChange={set("permitDate")} />
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

export function ResellableDialog({ plotId, onClose }: { plotId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const [reason, setReason] = useState("");
  const { busy, error, run } = useSubmit(onClose);
  return (
    <Dialog open onClose={onClose} title={t("plots.markResellable")} footer={<Footer danger onClose={onClose} busy={busy} disabled={reason.trim().length < 3} label={t("common.confirm")} onSubmit={() => run(() => api("plots.markResellable", { plotId, reason }))} />}>
      <div className="flex flex-col gap-4">
        <Alert tone="warn">{t("plots.resellableConfirm")}</Alert>
        <ReasonField value={reason} onChange={setReason} />
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

/** Urn release (JOG-15), urn removal, relocation. */
export function RemovalDialog({ detail, type, onClose }: { detail: PlotDetail; type: string; onClose: () => void }) {
  const { t } = useTranslation();
  const candidates = detail.occupants.filter((o) => (type === "relocation" ? true : o.container === "urn"));
  const [source, setSource] = useState(candidates[0]?.eventId ?? "");
  const [date, setDate] = useState(todayIso());
  const [receiver, setReceiver] = useState("");
  const [receiverAddress, setReceiverAddress] = useState("");
  const [destination, setDestination] = useState("");
  const [inside, setInside] = useState(true);
  const [targetIdent, setTargetIdent] = useState("");
  const [done, setDone] = useState<InterEvent | null>(null);
  const { busy, error, run } = useSubmit(() => {});
  const title = type === "urn_release" ? t("removal.titleRelease") : type === "urn_removal" ? t("removal.titleRemoval") : t("removal.titleRelocation");
  if (done) {
    return (
      <Dialog open onClose={onClose} title={title} footer={<Button variant="primary" onClick={onClose}>{t("common.close")}</Button>}>
        <div className="flex flex-col gap-4">
          <Alert tone="ok">{t("wizard.done", { folio: done.folio })}</Alert>
          {type === "urn_release" && <PrintButton doc="urn_receipt" args={{ eventId: done.id }} label={t("removal.printReceipt")} />}
        </div>
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={title}
      footer={
        <Footer
          onClose={onClose}
          busy={busy}
          label={t("removal.submit")}
          disabled={!source}
          onSubmit={() =>
            run(async () => {
              let targetPlotId: string | null = null;
              if (type === "relocation" && inside) {
                const p = await api<Plot | null>("plots.find", { cemeteryId: detail.plot.cemeteryId, identifier: targetIdent });
                if (!p) throw new ApiError({ code: "not_found", message: targetIdent });
                targetPlotId = p.id;
              }
              const ev = await api<InterEvent>("removal.record", {
                input: { eventType: type, eventDate: date, sourceEventId: source, receiverName: receiver, receiverAddress, destination, targetPlotId, historicalFolio: null, notes: "" },
              });
              setDone(ev);
            })
          }
        />
      }
    >
      <div className="flex flex-col gap-4">
        <RadioGroup label={t("removal.source")} value={source} onChange={setSource} options={candidates.map((c) => ({ value: c.eventId, label: c.deceasedName, hint: `${t(`container.${c.container}`)} · ${fmtDate(c.eventDate)}` }))} />
        <TextField label={t("common.date")} type="date" value={date} onChange={setDate} />
        {type === "urn_release" && (
          <div className="grid gap-4 md:grid-cols-2">
            <TextField label={t("fields.receiverName")} value={receiver} onChange={setReceiver} required />
            <TextField label={t("fields.receiverAddress")} value={receiverAddress} onChange={setReceiverAddress} />
          </div>
        )}
        {type === "relocation" && (
          <>
            <Checkbox label={t("removal.targetInside")} checked={inside} onChange={setInside} />
            {inside ? <TextField label={t("removal.targetPlot")} value={targetIdent} onChange={(v) => setTargetIdent(v.toUpperCase())} /> : <TextField label={t("fields.destination")} value={destination} onChange={setDestination} required />}
          </>
        )}
        {type !== "relocation" && <TextField label={t("fields.destination")} value={destination} onChange={setDestination} />}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

/** FR-TEM-06, FR-EML-03, FR-SIR-01: plot attributes. */
export function EditPlotDialog({ detail, onClose }: { detail: PlotDetail; onClose: () => void }) {
  const { t } = useTranslation();
  const types = usePlotTypes();
  const [p, setP] = useState<Plot>(detail.plot);
  const [reason, setReason] = useState("");
  const { busy, error, run } = useSubmit(onClose);
  const num = (v: string) => (v === "" ? null : Number(v));
  const needsReason = p.statusOverride !== detail.plot.statusOverride || p.protected !== detail.plot.protected;
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={t("plots.edit")}
      footer={<Footer onClose={onClose} busy={busy} label={t("common.saveChanges")} disabled={needsReason && reason.trim().length < 3} onSubmit={() => run(() => api("plots.update", { plot: p, reason: reason || null }))} />}
    >
      <div className="flex flex-col gap-4">
        <div className="grid gap-4 md:grid-cols-3">
          <Field label={t("fields.plotType")}>
            <Select value={p.plotTypeId} onChange={(e) => setP({ ...p, plotTypeId: e.target.value })}>
              {(types.data ?? []).map((x) => (
                <option key={x.id} value={x.id}>
                  {x.name}
                </option>
              ))}
            </Select>
          </Field>
          <TextField label={`${t("plots.capacity")}: ${t("settings.coffins").toLowerCase()}`} type="number" value={p.coffinCapacity} onChange={(v) => setP({ ...p, coffinCapacity: Number(v) })} />
          <TextField label={`${t("plots.capacity")}: ${t("settings.urns").toLowerCase()}`} type="number" value={p.urnCapacity} onChange={(v) => setP({ ...p, urnCapacity: Number(v) })} />
          <TextField label={`${t("plots.size")} (cm, szélesség)`} type="number" value={p.widthCm ?? ""} onChange={(v) => setP({ ...p, widthCm: num(v) })} />
          <TextField label={`${t("plots.size")} (cm, hossz)`} type="number" value={p.lengthCm ?? ""} onChange={(v) => setP({ ...p, lengthCm: num(v) })} />
          <TextField label={`${t("plots.restPeriod")} (év)`} type="number" value={p.restPeriodYears ?? ""} onChange={(v) => setP({ ...p, restPeriodYears: num(v) })} hint={t("common.optional")} />
        </div>
        <Checkbox label={t("plots.unusable")} checked={p.unusable} onChange={(v) => setP({ ...p, unusable: v })} />
        <Checkbox label={t("plots.protected")} checked={p.protected} onChange={(v) => setP({ ...p, protected: v, protectionKind: v ? p.protectionKind ?? "local" : null })} />
        {p.protected && (
          <div className="grid gap-4 md:grid-cols-2">
            <Field label={t("plots.protectionKind")}>
              <Select value={p.protectionKind ?? "local"} onChange={(e) => setP({ ...p, protectionKind: e.target.value as "national" | "local" })}>
                <option value="national">{t("plots.protectedNational")}</option>
                <option value="local">{t("plots.protectedLocal")}</option>
              </Select>
            </Field>
            <TextField label={t("plots.protectionReason")} value={p.protectionReason ?? ""} onChange={(v) => setP({ ...p, protectionReason: v })} />
          </div>
        )}
        <Checkbox label={t("plots.furtherBurial")} checked={p.furtherBurialAllowed} onChange={(v) => setP({ ...p, furtherBurialAllowed: v })} />
        {detail.plotType.category === "crypt" && (
          <div className="grid gap-4 md:grid-cols-3">
            <TextField label={t("plots.cryptNumber")} value={p.cryptNumber ?? ""} onChange={(v) => setP({ ...p, cryptNumber: v })} />
            <TextField label={t("plots.cryptBuiltYear")} type="number" value={p.cryptBuiltYear ?? ""} onChange={(v) => setP({ ...p, cryptBuiltYear: num(v) })} />
            <TextField label={t("plots.cryptPermit")} value={p.cryptPermit ?? ""} onChange={(v) => setP({ ...p, cryptPermit: v })} />
            <TextField className="md:col-span-3" label={t("plots.cryptInstructions")} value={p.cryptHolderInstructions ?? ""} onChange={(v) => setP({ ...p, cryptHolderInstructions: v })} />
          </div>
        )}
        <Field label={t("plots.statusOverride")}>
          <Select value={p.statusOverride ?? ""} onChange={(e) => setP({ ...p, statusOverride: e.target.value || null })}>
            <option value="">{t("plots.statusAuto")}</option>
            {["free", "reserved", "occupied", "expired", "notified", "resellable", "liquidated", "unusable"].map((s) => (
              <option key={s} value={s}>
                {t(`status.${s}`)}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("common.notes")}>
          <Input value={p.notes} onChange={(e) => setP({ ...p, notes: e.target.value })} />
        </Field>
        {needsReason && <ReasonField value={reason} onChange={setReason} />}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}
