import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Pencil, Ban } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import type { Deceased, EventRow, InterEvent } from "../../lib/types";
import { Button, DefinitionList, Dialog, ErrorAlert, PageHeader, Panel, Plaque, Spinner, TextField, cx } from "../../components/ui";
import { DeceasedForm, ReasonField } from "../../components/forms";
import { fmtDate, fullName } from "../../lib/format";
import { useInvalidate } from "../../lib/queries";
import { useToast } from "../../app/toast";
import { PrintButton } from "../prints/PrintButton";

/** Deceased data sheet with all register entries (FR-KER-05: jump to plot/map). */
export function DeceasedView({ id }: { id: string }) {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const d = useQuery({ queryKey: ["deceased", id], queryFn: () => api<Deceased>("deceased.get", { id }) });
  const ev = useQuery({ queryKey: ["deceasedEvents", id], queryFn: () => api<EventRow[]>("deceased.events", { id }) });
  const [editing, setEditing] = useState(false);
  const [voiding, setVoiding] = useState<EventRow | null>(null);
  const [correcting, setCorrecting] = useState<EventRow | null>(null);
  if (!d.data) return <Spinner />;
  const x = d.data;
  const activePlot = ev.data?.filter((e) => !e.voidedAt && e.plotId).slice(-1)[0];
  return (
    <>
      <PageHeader
        title={fullName(x) || t("fields.unknownIdentity")}
        subtitle={`${fmtDate(x.birthDate)} – ${fmtDate(x.deathDate)}`}
        actions={
          <>
            {activePlot?.plotId && (
              <>
                <Button onClick={() => nav.go({ view: "plot", id: activePlot.plotId! })}>{t("wizard.openPlot")}</Button>
                <Button onClick={() => nav.go({ view: "map", plotId: activePlot.plotId! })}>{t("plots.showOnMap")}</Button>
                <PrintButton doc="information" args={{ deceasedId: id }} label={t("prints.title") + ": felvilágosítás"} />
              </>
            )}
            {can("record_events") && (
              <Button icon={<Pencil className="size-5" aria-hidden />} onClick={() => setEditing(true)}>
                {t("common.edit")}
              </Button>
            )}
          </>
        }
      />
      <div className="grid gap-6 lg:grid-cols-2">
        <Panel title={t("wizard.steps.deceased")}>
          <DefinitionList
            items={[
              [t("fields.birthName"), x.birthName || "–"],
              [t("fields.sex"), t(`sex.${x.sex}`)],
              [t("fields.birthPlace"), x.birthPlace || "–"],
              [t("fields.birthDate"), fmtDate(x.birthDate)],
              [t("fields.motherName"), x.motherName || "–"],
              [t("fields.lastResidence"), x.lastResidence || "–"],
              [t("fields.deathPlace"), x.deathPlace || "–"],
              [t("fields.deathDate"), fmtDate(x.deathDate)],
              [t("fields.deathCertNo"), x.deathCertNo || "–"],
              ...(x.unknownIdentity ? ([[t("fields.authorityName"), x.authorityName], [t("fields.caseNumber"), x.caseNumber]] as [string, string][]) : []),
            ]}
          />
        </Panel>
        <Panel title={t("plots.history")}>
          {!ev.data ? (
            <Spinner />
          ) : (
            <ul className="divide-y divide-seam">
              {ev.data.map((e) => (
                <li key={e.id} className={cx("flex flex-wrap items-center justify-between gap-3 py-3", e.voidedAt && "text-granite")}>
                  <div className={cx(e.voidedAt && "line-through")}>
                    <p className="font-semibold">
                      {e.folio}. {t(`eventType.${e.eventType}`)}
                    </p>
                    <p className="text-sm text-granite">
                      {fmtDate(e.eventDate)}, {e.cemeteryName}
                      {e.arrangerName ? `, ${t("fields.arranger")}: ${e.arrangerName}` : ""}
                      {e.receiverName ? `, ${t("fields.receiverName")}: ${e.receiverName}` : ""}
                    </p>
                    {e.voidedAt && <p className="text-sm no-underline">{t("common.voided")}: {e.voidReason}</p>}
                  </div>
                  <div className="flex items-center gap-2">
                    {e.plotIdentifier && <Plaque identifier={e.plotIdentifier} />}
                    {!e.voidedAt && can("correct_entries") && (
                      <Button variant="quiet" onClick={() => setCorrecting(e)} aria-label={t("common.edit")}>
                        <Pencil className="size-4" />
                      </Button>
                    )}
                    {!e.voidedAt && can("void_entries") && (
                      <Button variant="quiet" onClick={() => setVoiding(e)} aria-label={t("common.void")}>
                        <Ban className="size-4" />
                      </Button>
                    )}
                    {e.eventType === "urn_release" && !e.voidedAt && <PrintButton doc="urn_receipt" args={{ eventId: e.id }} label={t("removal.printReceipt")} variant="quiet" />}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>
      {editing && <EditDeceased deceased={x} inRegister={(ev.data?.filter((e) => !e.voidedAt).length ?? 0) > 0} onClose={() => setEditing(false)} />}
      {voiding && <VoidEventDialog event={voiding} onClose={() => setVoiding(null)} />}
      {correcting && <CorrectEventDialog row={correcting} onClose={() => setCorrecting(null)} />}
    </>
  );
}

function EditDeceased({ deceased, inRegister, onClose }: { deceased: Deceased; inRegister: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const [v, setV] = useState<Partial<Deceased>>(deceased);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const invalidate = useInvalidate();
  const toast = useToast();
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={t("common.edit")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            busy={busy}
            onClick={async () => {
              setBusy(true);
              setError(null);
              try {
                await api("deceased.update", { deceased: { ...v, birthDate: v.birthDate || null, deathDate: v.deathDate || null }, reason: reason || null });
                await invalidate();
                toast(t("common.savedToast"));
                onClose();
              } catch (e) {
                setError(e);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("common.saveChanges")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <DeceasedForm value={v} onChange={setV} />
        {inRegister && <ReasonField value={reason} onChange={setReason} />}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

export function VoidEventDialog({ event, onClose }: { event: EventRow; onClose: () => void }) {
  const { t } = useTranslation();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("common.voidReasonTitle")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="danger"
            disabled={reason.trim().length < 3}
            onClick={async () => {
              try {
                await api("events.void", { id: event.id, reason });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.void")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <p>
          <strong>{event.folio}.</strong> {t(`eventType.${event.eventType}`)}, {event.deceasedName}, {fmtDate(event.eventDate)}
        </p>
        <p className="text-granite">{t("common.voidExplain")}</p>
        <ReasonField value={reason} onChange={setReason} />
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

function CorrectEventDialog({ row, onClose }: { row: EventRow; onClose: () => void }) {
  const { t } = useTranslation();
  const [date, setDate] = useState(row.eventDate);
  const [notes, setNotes] = useState(row.notes);
  const [receiver, setReceiver] = useState(row.receiverName);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  return (
    <Dialog
      open
      onClose={onClose}
      title={`${row.folio}. ${t("fields.folio").toLowerCase()} – ${t("common.edit").toLowerCase()}`}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={reason.trim().length < 3}
            onClick={async () => {
              try {
                const raw = await api<InterEvent>("events.raw", { id: row.id });
                await api("events.correct", { event: { ...raw, eventDate: date, notes, receiverName: receiver }, reason });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.saveChanges")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <TextField label={t("fields.eventDate")} type="date" value={date} onChange={setDate} />
        {row.eventType === "urn_release" && <TextField label={t("fields.receiverName")} value={receiver} onChange={setReceiver} />}
        <TextField label={t("common.notes")} value={notes} onChange={setNotes} />
        <ReasonField value={reason} onChange={setReason} />
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}
