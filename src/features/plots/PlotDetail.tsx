import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { BookPlus, MapPin, Pencil, ShieldCheck } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import type { EventRow, PlotDetail } from "../../lib/types";
import { Alert, Button, DefinitionList, Empty, PageHeader, Panel, Plaque, Spinner, StatusBadge, cx } from "../../components/ui";
import { fmtDate, fmtHuf } from "../../lib/format";
import { PrintButton } from "../prints/PrintButton";
import { VoidEventDialog } from "../burials/DeceasedView";
import { EditPlotDialog, HolderDialog, LeaseDialog, MonumentDialog, RemovalDialog, ResellableDialog, RestrictionDialog } from "./PlotDialogs";

type DialogKind = null | "edit" | "purchase" | "prepurchase" | "renewal" | "holder" | "contact" | "restriction" | "monument" | "resellable" | { removal: string; source?: string };

export function PlotDetailView({ id }: { id: string }) {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const { data, isLoading } = useQuery({ queryKey: ["plot", id], queryFn: () => api<PlotDetail>("plots.get", { id }) });
  const [dialog, setDialog] = useState<DialogKind>(null);
  const [voiding, setVoiding] = useState<EventRow | null>(null);
  if (isLoading || !data) return <Spinner />;
  const d = data;
  const p = d.plot;
  const hasUrns = d.occupants.some((o) => o.container === "urn");
  const empty = d.occupants.length === 0;
  const canBury = can("record_events");
  const canLease = can("record_leases");
  const primary = d.holders.filter((h) => h.role === "primary" && !h.endDate);
  return (
    <>
      <PageHeader
        title={
          <span className="flex flex-wrap items-center gap-4">
            <Plaque identifier={p.identifier} large />
            <span className="text-lg font-normal text-granite">{d.plotType.name}</span>
          </span>
        }
        subtitle={
          <span className="mt-2 flex flex-wrap items-center gap-3">
            <StatusBadge status={d.status} protectedPlot={p.protected} />
            <span className="text-granite">{d.cemeteryName}</span>
          </span>
        }
        actions={
          <>
            {canBury && empty && ["free", "reserved"].includes(d.status) && (
              <Button variant="primary" size="lg" icon={<BookPlus className="size-5" aria-hidden />} onClick={() => nav.go({ view: "newBurial", plotId: id })}>
                {t("plots.newBurialHere")}
              </Button>
            )}
            {canBury && !empty && (
              <Button variant="primary" size="lg" icon={<BookPlus className="size-5" aria-hidden />} onClick={() => nav.go({ view: "newBurial", plotId: id, eventType: "reburial" })}>
                {t("plots.reburial")}
              </Button>
            )}
            <Button icon={<MapPin className="size-5" aria-hidden />} onClick={() => nav.go({ view: "map", plotId: id })}>
              {t("plots.showOnMap")}
            </Button>
          </>
        }
      />
      {d.restrictions.filter((r) => !r.voidedAt).map((r) => (
        <div key={r.id} className="mb-4">
          <Alert tone={r.kind === "ban" ? "error" : "warn"} title={t(`plots.${r.kind}`)}>
            {r.text}
          </Alert>
        </div>
      ))}
      {p.protected && (
        <div className="mb-4">
          <Alert tone="info" title={p.protectionKind === "national" ? t("plots.protectedNational") : t("plots.protectedLocal")}>
            <span className="flex items-center gap-2">
              <ShieldCheck className="size-5 text-gilt" aria-hidden /> {p.protectionReason}, {t("plots.furtherBurial")}: {p.furtherBurialAllowed ? t("common.yes") : t("common.no")}
            </span>
          </Alert>
        </div>
      )}
      {d.resaleAllowedFrom && (
        <div className="mb-4">
          <Alert tone="warn">{t("plots.resellableFrom", { date: fmtDate(d.resaleAllowedFrom) })}</Alert>
        </div>
      )}

      <div className="grid gap-6 xl:grid-cols-[3fr_2fr]">
        <div className="flex flex-col gap-6">
          <Panel
            title={d.plotType.category === "crypt" ? t("plots.cryptSlots") : t("plots.occupants")}
            actions={
              canBury &&
              !empty && (
                <>
                  {hasUrns && <Button onClick={() => setDialog({ removal: "urn_release" })}>{t("plots.urnRelease")}</Button>}
                  {hasUrns && <Button onClick={() => setDialog({ removal: "urn_removal" })}>{t("plots.urnRemoval")}</Button>}
                  <Button onClick={() => setDialog({ removal: "relocation" })}>{t("plots.relocation")}</Button>
                </>
              )
            }
          >
            {d.plotType.category === "crypt" ? (
              <ol className="grid gap-2 sm:grid-cols-2">
                {d.cryptSlots.map((s) => (
                  <li key={s.id} className={cx("rounded-md border px-3 py-2", s.occupantName ? "border-seam" : "border-dashed border-seam text-granite")}>
                    <span className="text-sm text-granite">{t("plots.slot", { no: s.slotNo })}</span>
                    <span className="block font-semibold">{s.occupantName ?? t("plots.slotFree")}</span>
                    {s.occupantDate && (
                      <span className="text-sm text-granite">
                        {fmtDate(s.occupantDate)}, {s.occupantFolio}.
                      </span>
                    )}
                  </li>
                ))}
              </ol>
            ) : empty ? (
              <Empty>{t("plots.noOccupants")}</Empty>
            ) : (
              <ul className="divide-y divide-seam">
                {d.occupants.map((o) => (
                  <li key={o.eventId}>
                    <button type="button" onClick={() => nav.go({ view: "deceased", id: o.deceasedId })} className="flex w-full items-center justify-between gap-3 py-3 text-left hover:bg-limestone/60">
                      <span>
                        <span className="block text-lg font-semibold">{o.deceasedName}</span>
                        <span className="text-granite">
                          {fmtDate(o.birthDate)} – {fmtDate(o.deathDate)}
                        </span>
                      </span>
                      <span className="text-right text-sm text-granite">
                        {t(`container.${o.container}`)}, {fmtDate(o.eventDate)}
                        <span className="block">
                          {o.folio}. {t("fields.folio").toLowerCase()}
                        </span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </Panel>

          <Panel title={t("plots.history")}>
            {d.events.length === 0 ? (
              <Empty>{t("burials.empty")}</Empty>
            ) : (
              <table className="w-full">
                <tbody>
                  {d.events.map((e) => (
                    <tr key={e.id} className={cx("border-b border-seam last:border-0", e.voidedAt && "text-granite line-through")}>
                      <td className="py-2 pr-3 font-semibold">{e.folio}.</td>
                      <td className="py-2 pr-3 whitespace-nowrap">{fmtDate(e.eventDate)}</td>
                      <td className="py-2 pr-3">{t(`eventType.${e.eventType}`)}</td>
                      <td className="py-2 pr-3">{e.deceasedName}</td>
                      <td className="py-2 text-right">
                        {!e.voidedAt && can("void_entries") && (
                          <Button variant="quiet" className="min-h-9 text-sm" onClick={() => setVoiding(e)}>
                            {t("common.void")}
                          </Button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Panel>

          <Panel title={t("plots.monuments")} actions={canBury && <Button onClick={() => setDialog("monument")}>{t("plots.addMonument")}</Button>}>
            {d.monuments.length === 0 ? (
              <p className="text-granite">{t("common.none")}</p>
            ) : (
              <ul className="flex flex-col gap-2">
                {d.monuments.map((m) => (
                  <li key={m.id}>
                    <strong>{fmtDate(m.entryDate)}</strong> – {m.description}
                    {m.contractor && <span className="text-granite">, {m.contractor}</span>}
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        </div>

        <div className="flex flex-col gap-6">
          <Panel
            title={t("plots.leases")}
            actions={
              canLease && (
                <>
                  {["free", "resellable"].includes(d.status) && <Button onClick={() => setDialog(empty ? "prepurchase" : "purchase")}>{empty ? t("plots.prepurchase") : t("plots.purchase")}</Button>}
                  {d.currentLease && <Button onClick={() => setDialog("renewal")}>{t("plots.renewal")}</Button>}
                </>
              )
            }
          >
            {d.currentLease ? (
              <DefinitionList
                items={[
                  [t(`leaseKind.${d.currentLease.kind}`), `${fmtDate(d.currentLease.startDate)} – ${fmtDate(d.currentLease.expiryDate)}`],
                  [t("lease.fee"), d.currentLease.feeExempt ? t("wizard.feeExempt") : fmtHuf(d.currentLease.feeHuf)],
                ]}
              />
            ) : (
              <p className="text-granite">{t("common.none")}</p>
            )}
            {d.leases.length > 1 && (
              <details className="mt-3">
                <summary className="cursor-pointer text-granite">{t("plots.history")}</summary>
                <ul className="mt-2 flex flex-col gap-1 text-sm">
                  {d.leases.map((l) => (
                    <li key={l.id} className={cx(l.voidedAt && "line-through")}>
                      {t(`leaseKind.${l.kind}`)}: {fmtDate(l.startDate)} – {fmtDate(l.expiryDate)}, {fmtHuf(l.feeHuf)}, {t(`leaseStatus.${l.status}`)}
                    </li>
                  ))}
                </ul>
              </details>
            )}
            {d.currentLease && (
              <div className="mt-3 flex flex-wrap gap-2">
                <PrintButton doc="lease_certificate" args={{ leaseId: d.currentLease.id }} label={t("wizard.printCertificate")} variant="quiet" />
                {["expired", "notified"].includes(d.status) && <PrintButton doc="expiry_notice" args={{ plotIds: [id] }} label={t("expiries.printLetters")} variant="quiet" />}
              </div>
            )}
            {canLease && d.status === "notified" && (
              <Button className="mt-3" variant="danger" onClick={() => setDialog("resellable")}>
                {t("plots.markResellable")}
              </Button>
            )}
          </Panel>

          <Panel
            title={t("plots.holders")}
            actions={
              canLease && (
                <>
                  <Button onClick={() => setDialog("holder")}>{t("plots.holderChange")}</Button>
                  <Button variant="quiet" onClick={() => setDialog("contact")}>
                    {t("plots.addContact")}
                  </Button>
                </>
              )
            }
          >
            {d.holders.length === 0 ? (
              <p className="text-granite">{t("common.none")}</p>
            ) : (
              <ul className="flex flex-col gap-3">
                {d.holders.map((h) => (
                  <li key={h.id} className={cx(h.endDate && "text-granite")}>
                    <button type="button" className="font-semibold hover:underline" onClick={() => nav.go({ view: "person", id: h.personId })}>
                      {h.personName}
                    </button>
                    <span className="block text-sm">
                      {t(`holder.role.${h.role}`)}
                      {h.relation && `, ${h.relation}`}, {h.endDate ? `${fmtDate(h.startDate)} – ${fmtDate(h.endDate)}` : t("plots.since", { date: fmtDate(h.startDate) })}
                      {h.legalBasis && `, ${h.legalBasis}`}
                    </span>
                    {!h.endDate && h.personAddress && <span className="block text-sm text-granite">{h.personAddress}</span>}
                  </li>
                ))}
              </ul>
            )}
            {primary.length === 0 && d.occupants.length > 0 && <p className="mt-2 text-sm text-warn">{t("errors.holder_required")}</p>}
          </Panel>

          <Panel title={t("plots.edit")} actions={can("manage_structure") && <Button icon={<Pencil className="size-4" aria-hidden />} onClick={() => setDialog("edit")}>{t("common.edit")}</Button>}>
            <DefinitionList
              items={
                [
                  [t("plots.capacity"), t("plots.capacityValue", { coffins: p.coffinCapacity, urns: p.urnCapacity })],
                  [t("plots.restPeriod"), t("common.years", { count: d.restPeriodYears })],
                  [t("plots.section") + " / " + t("plots.row").toLowerCase(), `${d.sectionCode} / ${d.rowNumber}`],
                  p.widthCm ? [t("plots.size"), `${p.widthCm} × ${p.lengthCm} cm`] : null,
                  p.cryptNumber ? [t("plots.cryptNumber"), p.cryptNumber] : null,
                  p.cryptBuiltYear ? [t("plots.cryptBuiltYear"), String(p.cryptBuiltYear)] : null,
                  p.cryptPermit ? [t("plots.cryptPermit"), p.cryptPermit] : null,
                  p.cryptHolderInstructions ? [t("plots.cryptInstructions"), p.cryptHolderInstructions] : null,
                  p.statusOverride ? [t("plots.statusOverride"), `${t(`status.${p.statusOverride}`)} – ${p.statusOverrideReason ?? ""}`] : null,
                  p.notes ? [t("common.notes"), p.notes] : null,
                ].filter(Boolean) as [ReactNode, ReactNode][]
              }
            />
            {can("record_events") && (
              <Button className="mt-3" variant="quiet" onClick={() => setDialog("restriction")}>
                {t("plots.addRestriction")}
              </Button>
            )}
          </Panel>

          {d.notices.length > 0 && (
            <Panel title={t("plots.notices")}>
              <ul className="flex flex-col gap-1">
                {d.notices.map((n) => (
                  <li key={n.id}>
                    {fmtDate(n.noticeDate)}, {t(`noticeKind.${n.kind}`)} {n.result && `, ${n.result}`}
                  </li>
                ))}
              </ul>
            </Panel>
          )}
        </div>
      </div>

      {dialog === "edit" && <EditPlotDialog detail={d} onClose={() => setDialog(null)} />}
      {(dialog === "purchase" || dialog === "prepurchase" || dialog === "renewal") && <LeaseDialog detail={d} kind={dialog} onClose={() => setDialog(null)} />}
      {(dialog === "holder" || dialog === "contact") && <HolderDialog plotId={id} role={dialog === "holder" ? "primary" : "contact"} onClose={() => setDialog(null)} />}
      {dialog === "restriction" && <RestrictionDialog plotId={id} onClose={() => setDialog(null)} />}
      {dialog === "monument" && <MonumentDialog plotId={id} onClose={() => setDialog(null)} />}
      {dialog === "resellable" && <ResellableDialog plotId={id} onClose={() => setDialog(null)} />}
      {dialog && typeof dialog === "object" && <RemovalDialog detail={d} type={dialog.removal} onClose={() => setDialog(null)} />}
      {voiding && <VoidEventDialog event={voiding} onClose={() => setVoiding(null)} />}
    </>
  );
}
