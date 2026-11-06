import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { CalendarPlus, Check, ChevronLeft, ChevronRight, FilePlus2, Paperclip, Plus, Printer, X } from "lucide-react";
import { api } from "../../lib/api";
import { useApp } from "../../app/context";
import { useCurrentCemetery, useInvalidate } from "../../lib/queries";
import { fmtDate, fmtDateTime, fmtHuf, todayIso } from "../../lib/format";
import { Alert, Button, Checkbox, Dialog, Empty, ErrorAlert, Field, PageHeader, Panel, Plaque, RadioGroup, Select, Spinner, Tabs, TextField, Textarea, cx } from "../../components/ui";
import { PlotPicker, ReasonField } from "../../components/forms";
import { PrintButton, openGenerated, type GeneratedFile } from "../prints/PrintButton";
import { InvoiceDialog, type Charge } from "../finance/Finance";

// ---------------------------------------------------------------- types (JSON from Postgres, camelCase)

interface Facility {
  id: string;
  name: string;
  kind: "chapel" | "cooler" | "hall" | "other";
  notes: string;
}
interface Booking {
  id: string;
  title: string;
  startsAt: string;
  endsAt: string;
  facilityId: string | null;
  facilityName: string | null;
  plotId: string | null;
  plotIdentifier: string | null;
  contractorId: string | null;
  contractorName: string | null;
  deceasedName: string;
  container: "" | "coffin" | "urn";
  status: "planned" | "done" | "cancelled";
  notes: string;
  createdBy: string | null;
}
interface WorkOrder {
  id: string;
  kind: string;
  title: string;
  plotId: string | null;
  plotIdentifier: string | null;
  bookingStartsAt: string | null;
  dueDate: string;
  assignee: string;
  details: string;
  recurrence: "none" | "weekly" | "monthly" | "yearly";
  status: "open" | "done" | "cancelled";
  doneAt: string | null;
  doneBy: string | null;
  doneNote: string;
}
interface Contractor {
  id: string;
  name: string;
  kind: "funeral" | "stonemason" | "gardener" | "other";
  taxNumber: string;
  postcode: string;
  city: string;
  address: string;
  contactName: string;
  phone: string;
  email: string;
  permitNumber: string;
  permitValidUntil: string | null;
  notes: string;
  permitExpired: boolean;
  notices: number;
}
interface WorkNotice {
  id: string;
  contractorName: string;
  plotIdentifier: string | null;
  kind: string;
  workDate: string;
  description: string;
  chargeGrossHuf: number | null;
  chargeStatus: string | null;
}
interface CaseRow {
  id: string;
  caseNumber: string;
  kind: string;
  subject: string;
  applicantName: string;
  plotId: string | null;
  plotIdentifier: string | null;
  contractorName: string | null;
  receivedOn: string;
  deadline: string | null;
  status: "received" | "missing_docs" | "approved" | "rejected" | "closed";
  decision: string;
  attachments: number;
}
interface Attachment {
  id: string;
  fileName: string;
  mimeType: string;
  size: number;
  createdAt: string;
  createdBy: string | null;
}
interface HistoryRow {
  ts: string;
  userName: string;
  action: string;
  newValue: string | null;
  reason: string | null;
}
interface ConditionRow {
  id: string;
  plotId: string;
  plotIdentifier: string;
  observedOn: string;
  condition: "ok" | "neglected" | "dangerous" | "to_liquidate";
  note: string;
  caseNumber: string | null;
}
interface Agenda {
  bookings: Booking[];
  workOrders: WorkOrder[];
  caseDeadlines: CaseRow[];
  today: string;
}

type Tab = "calendar" | "orders" | "cases" | "contractors" | "inspection";

const ORDER_KINDS = ["grave_digging", "preparation", "maintenance", "cleaning", "tree", "repair", "other"];
const CASE_KINDS = ["lease_contract", "renewal", "monument_permit", "monument_removal", "exhumation", "relocation", "ash_release", "holder_transfer", "neglect", "complaint", "other"];
const CASE_STATUSES = ["received", "missing_docs", "approved", "rejected", "closed"] as const;

const th = "py-2 pr-4 text-left text-sm font-medium text-granite";
const td = "py-2.5 pr-4 align-top";

// ---------------------------------------------------------------- dates

const pad = (n: number) => String(n).padStart(2, "0");
const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
function parseIso(s: string) {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}
function addDays(s: string, n: number) {
  const d = parseIso(s);
  d.setDate(d.getDate() + n);
  return iso(d);
}
function mondayOf(s: string) {
  const d = parseIso(s);
  const wd = (d.getDay() + 6) % 7;
  d.setDate(d.getDate() - wd);
  return iso(d);
}
const time = (local: string) => local.slice(11, 16);

/** W8–W11 – Üzemeltetés: funeral calendar, work orders, case files, contractors, inspection. */
export function OperationsView() {
  const { t } = useTranslation();
  const { can } = useApp();
  const { current } = useCurrentCemetery();
  const full = can("operations");
  // an invited funeral office sees the shared calendar, a field worker the work orders
  const tabs: Tab[] = full
    ? ["calendar", "orders", "cases", "contractors", "inspection"]
    : [...(can("calendar") ? (["calendar"] as Tab[]) : []), ...(can("complete_work") ? (["orders"] as Tab[]) : [])];
  const [tab, setTab] = useState<Tab>(tabs[0]);
  if (!current) return <Spinner />;
  return (
    <>
      <PageHeader title={t("ops.title")} subtitle={current.name} />
      {tabs.length > 1 && (
        <div className="no-print">
          <Tabs value={tab} onChange={setTab} items={tabs.map((x) => ({ value: x, label: t(`ops.tabs.${x}`) }))} />
        </div>
      )}
      <div className="mt-6">
        {tab === "calendar" && <CalendarTab cemeteryId={current.id} />}
        {tab === "orders" && <OrdersTab cemeteryId={current.id} manage={full} />}
        {tab === "cases" && <CasesTab cemeteryId={current.id} />}
        {tab === "contractors" && <ContractorsTab cemeteryId={current.id} />}
        {tab === "inspection" && <InspectionTab cemeteryId={current.id} />}
      </div>
    </>
  );
}

// ---------------------------------------------------------------- calendar (FR-SZE-01/02)

function CalendarTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const { can, nav, status } = useApp();
  const manager = can("operations");
  const me = status?.session?.userName ?? "";
  const mayEdit = (b: Booking) => b.status === "planned" && (manager || b.createdBy === me);
  const [week, setWeek] = useState(() => mondayOf(todayIso()));
  const to = addDays(week, 6);
  const agenda = useQuery({ queryKey: ["agenda", cemeteryId, week], queryFn: () => api<Agenda>("operations.agenda", { cemeteryId, from: week, to }) });
  const facilities = useQuery({ queryKey: ["facilities", cemeteryId], queryFn: () => api<Facility[]>("facilities.list", { cemeteryId }) });
  const [editing, setEditing] = useState<Partial<Booking> | null>(null);
  const [closing, setClosing] = useState<Booking | null>(null);
  const [facilityDialog, setFacilityDialog] = useState(false);
  const days = Array.from({ length: 7 }, (_, i) => addDays(week, i));
  const a = agenda.data;
  const today = todayIso();
  const overdue = (a?.workOrders ?? []).filter((w) => w.dueDate < week);
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Button className="no-print" aria-label={t("ops.prevWeek")} icon={<ChevronLeft className="size-5" aria-hidden />} onClick={() => setWeek(addDays(week, -7))} />
          <Button className="no-print" onClick={() => setWeek(mondayOf(today))}>{t("ops.thisWeek")}</Button>
          <Button className="no-print" aria-label={t("ops.nextWeek")} icon={<ChevronRight className="size-5" aria-hidden />} onClick={() => setWeek(addDays(week, 7))} />
          <span className="ml-2 font-semibold tabular-nums">
            {fmtDate(week)} – {fmtDate(to)}
          </span>
        </div>
        <div className="no-print flex flex-wrap gap-2">
          <Button icon={<Printer className="size-5" aria-hidden />} onClick={() => window.print()}>
            {t("ops.print")}
          </Button>
          {manager && <Button onClick={() => setFacilityDialog(true)}>{t("ops.facilities")}</Button>}
          <Button variant="primary" icon={<CalendarPlus className="size-5" aria-hidden />} onClick={() => setEditing({ startsAt: `${today}T10:00`, endsAt: `${today}T11:00` })} data-tour="new-booking">
            {t("ops.newBooking")}
          </Button>
        </div>
      </div>
      {overdue.length > 0 && <Alert tone="warn">{t("ops.overdueOrders", { count: overdue.length })}</Alert>}
      {!a ? (
        <Spinner />
      ) : (
        <ol className="grid gap-3 md:grid-cols-2 xl:grid-cols-7" data-testid="calendar">
          {days.map((d) => {
            const bookings = a.bookings.filter((b) => b.startsAt.slice(0, 10) === d);
            const orders = a.workOrders.filter((w) => w.dueDate === d);
            const deadlines = a.caseDeadlines.filter((c) => c.deadline === d);
            return (
              <li key={d} className={cx("flex min-h-40 flex-col gap-2 rounded-xl border bg-slab p-3", d === today ? "border-moss" : "border-seam")}>
                <h3 className={cx("flex items-baseline justify-between gap-2 text-sm font-semibold", d === today ? "text-moss" : "text-granite")}>
                  <span>{t(`ops.weekday.${(parseIso(d).getDay() + 6) % 7}`)}</span>
                  <span className="font-normal tabular-nums">{d.slice(5).replace("-", ". ")}.</span>
                </h3>
                {bookings.map((b) => (
                  <button
                    key={b.id}
                    type="button"
                    onClick={() => (mayEdit(b) ? setEditing(b) : undefined)}
                    className={cx(
                      "rounded-lg border px-2.5 py-2 text-left text-sm",
                      b.status === "cancelled" ? "border-seam text-granite line-through" : b.status === "done" ? "border-seam bg-lichen/40" : "border-moss/40 bg-moss/8 hover:bg-moss/12",
                    )}
                  >
                    <span className="block font-semibold tabular-nums">
                      {time(b.startsAt)}–{time(b.endsAt)}
                    </span>
                    <span className="block">{b.title}</span>
                    {(b.facilityName || b.plotIdentifier) && (
                      <span className="block text-granite">{[b.facilityName, b.plotIdentifier].filter(Boolean).join(" · ")}</span>
                    )}
                  </button>
                ))}
                {orders.map((w) => (
                  <p key={w.id} className="rounded-lg border border-dashed border-seam px-2.5 py-1.5 text-sm">
                    <span className="font-semibold">{t(`ops.orderKinds.${w.kind}`)}</span> {w.plotIdentifier ?? w.title}
                  </p>
                ))}
                {deadlines.map((c) => (
                  <p key={c.id} className="rounded-lg border border-warn/40 bg-warn-bg px-2.5 py-1.5 text-sm text-warn">
                    {t("ops.caseDeadline", { number: c.caseNumber })}
                  </p>
                ))}
              </li>
            );
          })}
        </ol>
      )}
      {a && (
        <Panel title={t("ops.weekList")}>
          {a.bookings.length === 0 ? (
            <Empty>{t("ops.noBookings")}</Empty>
          ) : (
            <table className="w-full min-w-[720px]">
              <thead>
                <tr className="border-b border-seam">
                  <th className={th}>{t("ops.when")}</th>
                  <th className={th}>{t("ops.bookingTitle")}</th>
                  <th className={th}>{t("ops.facility")}</th>
                  <th className={th}>{t("ops.plot")}</th>
                  <th className={th}>{t("ops.contractor")}</th>
                  <th className={th}>{t("ops.bookedBy")}</th>
                  <th className={th}>{t("common.status")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {a.bookings.map((b) => (
                  <tr key={b.id} className="border-b border-seam last:border-0">
                    <td className={cx(td, "tabular-nums")}>
                      {fmtDate(b.startsAt.slice(0, 10))} {time(b.startsAt)}–{time(b.endsAt)}
                    </td>
                    <td className={cx(td, "font-semibold")}>{b.title}</td>
                    <td className={td}>{b.facilityName ?? "–"}</td>
                    <td className={td}>
                      <Plaque identifier={b.plotIdentifier} />
                    </td>
                    <td className={td}>{b.contractorName ?? "–"}</td>
                    <td className={cx(td, "text-sm text-granite")}>{b.createdBy === me ? t("ops.bookedByMe") : (b.createdBy ?? "–")}</td>
                    <td className={td}>{t(`ops.bookingStatus.${b.status}`)}</td>
                    <td className={cx(td, "text-right")}>
                      {mayEdit(b) && (
                        <div className="flex justify-end gap-2">
                          {can("record_events") && (
                            <Button size="md" variant="primary" onClick={() => nav.go({ view: "newBurial", plotId: b.plotId ?? undefined, bookingId: b.id })}>
                              {t("ops.recordBurial")}
                            </Button>
                          )}
                          <Button size="md" onClick={() => setClosing(b)}>
                            {t("ops.close")}
                          </Button>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Panel>
      )}
      {editing && <BookingDialog cemeteryId={cemeteryId} booking={editing} facilities={facilities.data ?? []} manager={manager} onClose={() => setEditing(null)} />}
      {closing && <CloseBookingDialog booking={closing} manager={manager} onClose={() => setClosing(null)} />}
      {facilityDialog && <FacilitiesDialog cemeteryId={cemeteryId} facilities={facilities.data ?? []} onClose={() => setFacilityDialog(false)} />}
    </div>
  );
}

function BookingDialog({ cemeteryId, booking, facilities, manager, onClose }: { cemeteryId: string; booking: Partial<Booking>; facilities: Facility[]; manager: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const contractors = useQuery({ queryKey: ["contractors"], queryFn: () => api<Contractor[]>("contractors.list"), enabled: manager });
  const [v, setV] = useState({
    title: booking.title ?? "",
    date: (booking.startsAt ?? todayIso()).slice(0, 10),
    start: time(booking.startsAt ?? "T10:00"),
    end: time(booking.endsAt ?? "T11:00"),
    facilityId: booking.facilityId ?? facilities[0]?.id ?? "",
    plotId: booking.plotId ?? null,
    contractorId: booking.contractorId ?? "",
    deceasedName: booking.deceasedName ?? "",
    container: booking.container ?? "",
    notes: booking.notes ?? "",
    digOrder: !booking.id,
  });
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const set = (k: keyof typeof v) => (x: string) => setV({ ...v, [k]: x });
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={booking.id ? t("ops.editBooking") : t("ops.newBooking")}
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
                await api("bookings.save", {
                  booking: {
                    id: booking.id ?? null,
                    cemeteryId,
                    title: v.title,
                    startsAt: `${v.date}T${v.start}`,
                    endsAt: `${v.date}T${v.end}`,
                    facilityId: v.facilityId || null,
                    plotId: v.plotId,
                    contractorId: v.contractorId || null,
                    deceasedName: v.deceasedName,
                    container: v.container,
                    notes: v.notes,
                    digOrder: v.digOrder && !!v.plotId,
                  },
                });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-3">
        <TextField className="md:col-span-3" label={t("ops.bookingTitle")} value={v.title} onChange={set("title")} required hint={t("ops.bookingTitleHint")} inputProps={{ name: "title" }} />
        <TextField label={t("common.date")} type="date" value={v.date} onChange={set("date")} required />
        <TextField label={t("ops.start")} type="time" value={v.start} onChange={set("start")} required inputProps={{ name: "start" }} />
        <TextField label={t("ops.end")} type="time" value={v.end} onChange={set("end")} required inputProps={{ name: "end" }} />
        <Field label={t("ops.facility")}>
          <Select value={v.facilityId} onChange={(e) => set("facilityId")(e.target.value)} name="facility">
            <option value="">{t("ops.noFacility")}</option>
            {facilities.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </Select>
        </Field>
        {manager && (
          <Field label={t("ops.contractor")}>
            <Select value={v.contractorId} onChange={(e) => set("contractorId")(e.target.value)}>
              <option value="">–</option>
              {(contractors.data ?? []).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </Select>
          </Field>
        )}
        <Field label={t("ops.container")}>
          <Select value={v.container} onChange={(e) => set("container")(e.target.value)}>
            <option value="">–</option>
            <option value="coffin">{t("ops.containers.coffin")}</option>
            <option value="urn">{t("ops.containers.urn")}</option>
          </Select>
        </Field>
        <TextField className="md:col-span-3" label={t("ops.deceasedName")} value={v.deceasedName} onChange={set("deceasedName")} />
        <div className="md:col-span-3">
          <PlotPicker cemeteryId={cemeteryId} value={v.plotId} onChange={(id) => setV({ ...v, plotId: id })} />
        </div>
        {!booking.id && manager && <Checkbox label={t("ops.digOrder")} hint={t("ops.digOrderHint")} checked={v.digOrder} onChange={(x) => setV({ ...v, digOrder: x })} disabled={!v.plotId} />}
        <Field label={t("common.notes")} className="md:col-span-3">
          <Textarea value={v.notes} onChange={(e) => set("notes")(e.target.value)} rows={2} />
        </Field>
      </div>
      <div className="mt-4">
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

function CloseBookingDialog({ booking, manager, onClose }: { booking: Booking; manager: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [status, setStatus] = useState<"done" | "cancelled">(manager ? "done" : "cancelled");
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  return (
    <Dialog
      open
      onClose={onClose}
      title={booking.title}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              setError(null);
              try {
                await api("bookings.status", { id: booking.id, status, reason: status === "cancelled" ? reason : null });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <RadioGroup
          label={t("ops.closeAs")}
          value={status}
          onChange={setStatus}
          options={[
            ...(manager ? [{ value: "done" as const, label: t("ops.bookingStatus.done"), hint: t("ops.doneHint") }] : []),
            { value: "cancelled" as const, label: t("ops.bookingStatus.cancelled"), hint: t("ops.cancelHint") },
          ]}
        />
        {status === "cancelled" && <ReasonField value={reason} onChange={setReason} />}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

function FacilitiesDialog({ cemeteryId, facilities, onClose }: { cemeteryId: string; facilities: Facility[]; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [name, setName] = useState("");
  const [kind, setKind] = useState<Facility["kind"]>("chapel");
  const [error, setError] = useState<unknown>(null);
  return (
    <Dialog open onClose={onClose} title={t("ops.facilities")} footer={<Button onClick={onClose}>{t("common.close")}</Button>}>
      <div className="flex flex-col gap-4">
        <p className="text-granite">{t("ops.facilitiesHint")}</p>
        {facilities.length > 0 && (
          <ul className="divide-y divide-seam rounded-lg border border-seam">
            {facilities.map((f) => (
              <li key={f.id} className="flex items-center justify-between px-3 py-2">
                <span>
                  <span className="font-semibold">{f.name}</span> <span className="text-granite">· {t(`ops.facilityKinds.${f.kind}`)}</span>
                </span>
                <Button
                  aria-label={t("common.delete")}
                  icon={<X className="size-4" aria-hidden />}
                  onClick={async () => {
                    try {
                      await api("facilities.void", { id: f.id, reason: "megszűnt" });
                      await invalidate();
                    } catch (e) {
                      setError(e);
                    }
                  }}
                />
              </li>
            ))}
          </ul>
        )}
        <div className="grid gap-3 sm:grid-cols-[1fr_auto_auto] sm:items-end">
          <TextField label={t("ops.facilityName")} value={name} onChange={setName} inputProps={{ name: "facilityName" }} />
          <Field label={t("ops.facilityKind")}>
            <Select value={kind} onChange={(e) => setKind(e.target.value as Facility["kind"])}>
              {(["chapel", "cooler", "hall", "other"] as const).map((k) => (
                <option key={k} value={k}>
                  {t(`ops.facilityKinds.${k}`)}
                </option>
              ))}
            </Select>
          </Field>
          <Button
            icon={<Plus className="size-4" aria-hidden />}
            onClick={async () => {
              setError(null);
              try {
                await api("facilities.save", { facility: { cemeteryId, name, kind } });
                setName("");
                await invalidate();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.add")}
          </Button>
        </div>
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------- work orders (FR-SZE-03, FR-UZE-01)

function OrdersTab({ cemeteryId, manage }: { cemeteryId: string; manage: boolean }) {
  const { t } = useTranslation();
  const [show, setShow] = useState<"open" | "done">("open");
  const q = useQuery({ queryKey: ["workOrders", cemeteryId, show], queryFn: () => api<WorkOrder[]>("workOrders.list", { cemeteryId, status: show }) });
  const [editing, setEditing] = useState<Partial<WorkOrder> | null>(null);
  const [completing, setCompleting] = useState<WorkOrder | null>(null);
  const today = todayIso();
  const rows = q.data ?? [];
  return (
    <div className="flex flex-col gap-4">
      <div className="no-print flex flex-wrap items-center justify-between gap-3">
        <div className="flex gap-2" role="group">
          {(["open", "done"] as const).map((s) => (
            <Button key={s} variant={show === s ? "primary" : "secondary"} onClick={() => setShow(s)} aria-pressed={show === s}>
              {t(`ops.orderShow.${s}`)}
            </Button>
          ))}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button icon={<Printer className="size-5" aria-hidden />} onClick={() => window.print()}>
            {t("ops.print")}
          </Button>
          {manage && (
            <Button variant="primary" icon={<Plus className="size-5" aria-hidden />} onClick={() => setEditing({ dueDate: today, kind: "maintenance", recurrence: "none" })}>
              {t("ops.newOrder")}
            </Button>
          )}
        </div>
      </div>
      {!q.data ? (
        <Spinner />
      ) : rows.length === 0 ? (
        <Empty>{show === "open" ? t("ops.noOpenOrders") : t("ops.noDoneOrders")}</Empty>
      ) : (
        <ul className="grid gap-3 lg:grid-cols-2" data-testid="work-orders">
          {rows.map((w) => {
            const late = w.status === "open" && w.dueDate < today;
            return (
              <li key={w.id} className={cx("flex flex-col gap-3 rounded-xl border bg-slab p-4", late ? "border-warn/60" : "border-seam")}>
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-sm font-semibold text-granite">
                      {t(`ops.orderKinds.${w.kind}`)}
                      {w.recurrence !== "none" && ` · ${t(`ops.recurrence.${w.recurrence}`)}`}
                    </p>
                    <h3 className="text-lg">{w.title}</h3>
                  </div>
                  {w.plotIdentifier && <Plaque identifier={w.plotIdentifier} />}
                </div>
                <p className={cx("tabular-nums", late && "font-semibold text-warn")}>
                  {late ? t("ops.lateSince", { date: fmtDate(w.dueDate) }) : t("ops.dueOn", { date: fmtDate(w.dueDate) })}
                  {w.bookingStartsAt && ` · ${t("ops.ceremonyAt", { time: time(w.bookingStartsAt) })}`}
                  {w.assignee && ` · ${w.assignee}`}
                </p>
                {w.details && <p className="whitespace-pre-line">{w.details}</p>}
                {w.status !== "open" ? (
                  <p className="text-sm text-granite">
                    {t(`ops.orderStatus.${w.status}`)} {w.doneAt && fmtDateTime(w.doneAt)} {w.doneBy && `· ${w.doneBy}`} {w.doneNote && `· ${w.doneNote}`}
                  </p>
                ) : (
                  <div className="no-print flex flex-wrap gap-2">
                    <Button variant="primary" size="lg" icon={<Check className="size-5" aria-hidden />} onClick={() => setCompleting(w)}>
                      {t("ops.markDone")}
                    </Button>
                    {manage && <Button onClick={() => setEditing(w)}>{t("common.edit")}</Button>}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {editing && <OrderDialog cemeteryId={cemeteryId} order={editing} onClose={() => setEditing(null)} />}
      {completing && <CompleteDialog order={completing} manage={manage} onClose={() => setCompleting(null)} />}
    </div>
  );
}

function OrderDialog({ cemeteryId, order, onClose }: { cemeteryId: string; order: Partial<WorkOrder>; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [v, setV] = useState({
    kind: order.kind ?? "maintenance",
    title: order.title ?? "",
    dueDate: order.dueDate ?? todayIso(),
    assignee: order.assignee ?? "",
    details: order.details ?? "",
    recurrence: order.recurrence ?? "none",
    plotId: order.plotId ?? null,
  });
  const [error, setError] = useState<unknown>(null);
  const [withPlot, setWithPlot] = useState(!!order.plotId);
  const set = (k: keyof typeof v) => (x: string) => setV({ ...v, [k]: x });
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={order.id ? t("ops.editOrder") : t("ops.newOrder")}
      footer={
        <>
          {order.id && (
            <Button
              variant="quiet"
              onClick={async () => {
                const reason = window.prompt(t("common.reason")) ?? "";
                if (!reason) return;
                try {
                  await api("workOrders.cancel", { id: order.id, reason });
                  await invalidate();
                  onClose();
                } catch (e) {
                  setError(e);
                }
              }}
            >
              {t("ops.cancelOrder")}
            </Button>
          )}
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              setError(null);
              try {
                await api("workOrders.save", { order: { id: order.id ?? null, cemeteryId, ...v, plotId: withPlot ? v.plotId : null } });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        <Field label={t("ops.orderKind")}>
          <Select value={v.kind} onChange={(e) => set("kind")(e.target.value)} disabled={!!order.id} name="kind">
            {ORDER_KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`ops.orderKinds.${k}`)}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("ops.recurrenceLabel")}>
          <Select value={v.recurrence} onChange={(e) => set("recurrence")(e.target.value)} name="recurrence">
            {(["none", "weekly", "monthly", "yearly"] as const).map((k) => (
              <option key={k} value={k}>
                {t(`ops.recurrence.${k}`)}
              </option>
            ))}
          </Select>
        </Field>
        <TextField className="md:col-span-2" label={t("ops.orderTitle")} value={v.title} onChange={set("title")} required inputProps={{ name: "title" }} />
        <TextField label={t("ops.dueDate")} type="date" value={v.dueDate} onChange={set("dueDate")} required />
        <TextField label={t("ops.assignee")} value={v.assignee} onChange={set("assignee")} hint={t("ops.assigneeHint")} />
        <Field label={t("ops.details")} className="md:col-span-2">
          <Textarea value={v.details} onChange={(e) => set("details")(e.target.value)} rows={3} />
        </Field>
        <div className="md:col-span-2 flex flex-col gap-3">
          <Checkbox label={t("ops.forPlot")} checked={withPlot} onChange={setWithPlot} />
          {withPlot && <PlotPicker cemeteryId={cemeteryId} value={v.plotId} onChange={(id) => setV({ ...v, plotId: id })} />}
        </div>
      </div>
      <div className="mt-4">
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

function CompleteDialog({ order, manage, onClose }: { order: WorkOrder; manage: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [note, setNote] = useState("");
  const [photo, setPhoto] = useState<File | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  return (
    <Dialog
      open
      onClose={onClose}
      title={order.title}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            busy={busy}
            icon={<Check className="size-5" aria-hidden />}
            onClick={async () => {
              setBusy(true);
              setError(null);
              try {
                if (photo) await uploadFile("work_order", order.id, photo);
                await api("workOrders.complete", { id: order.id, note });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("ops.markDone")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {order.recurrence !== "none" && <Alert>{t("ops.nextWillBeCreated")}</Alert>}
        <Field label={t("ops.doneNote")}>
          <Textarea value={note} onChange={(e) => setNote(e.target.value)} rows={3} name="note" />
        </Field>
        <Field label={t("ops.photo")} hint={manage ? undefined : t("ops.photoHint")}>
          <input type="file" accept="image/*,application/pdf" capture="environment" onChange={(e) => setPhoto(e.target.files?.[0] ?? null)} />
        </Field>
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------- attachments

const MAX_UPLOAD = 3 * 1024 * 1024;

export async function uploadFile(entity: string, entityId: string, file: File) {
  if (file.size > MAX_UPLOAD) {
    const { ApiError } = await import("../../lib/api");
    throw new ApiError({ code: "file_too_large", message: "file too large", params: { maxMb: "3" } });
  }
  const buf = new Uint8Array(await file.arrayBuffer());
  let bin = "";
  for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
  await api("attachments.add", { entity, entityId, fileName: file.name, mimeType: file.type, content: btoa(bin) });
}

const fmtSize = (n: number) => (n > 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} kB`);

// ---------------------------------------------------------------- cases (FR-ENG-01/02)

function CasesTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const [status, setStatus] = useState("");
  const q = useQuery({ queryKey: ["cases", cemeteryId, status], queryFn: () => api<CaseRow[]>("cases.list", { cemeteryId, status: status || null }) });
  const [creating, setCreating] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const today = todayIso();
  const rows = q.data ?? [];
  return (
    <Panel
      title={t("ops.cases")}
      actions={
        <>
          <Select value={status} onChange={(e) => setStatus(e.target.value)} aria-label={t("common.status")}>
            <option value="">{t("ops.allStatuses")}</option>
            {CASE_STATUSES.map((s) => (
              <option key={s} value={s}>
                {t(`ops.caseStatus.${s}`)}
              </option>
            ))}
          </Select>
          <Button variant="primary" icon={<FilePlus2 className="size-5" aria-hidden />} onClick={() => setCreating(true)}>
            {t("ops.newCase")}
          </Button>
        </>
      }
    >
      {!q.data ? (
        <Spinner />
      ) : rows.length === 0 ? (
        <Empty>{t("ops.noCases")}</Empty>
      ) : (
        <table className="w-full min-w-[760px]" data-testid="cases">
          <thead>
            <tr className="border-b border-seam">
              <th className={th}>{t("ops.caseNumber")}</th>
              <th className={th}>{t("ops.subject")}</th>
              <th className={th}>{t("ops.applicant")}</th>
              <th className={th}>{t("ops.plot")}</th>
              <th className={th}>{t("ops.received")}</th>
              <th className={th}>{t("ops.deadline")}</th>
              <th className={th}>{t("common.status")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((c) => {
              const late = c.deadline && c.deadline < today && (c.status === "received" || c.status === "missing_docs");
              return (
                <tr key={c.id} className="cursor-pointer border-b border-seam last:border-0 hover:bg-lichen/30" onClick={() => setOpen(c.id)}>
                  <td className={cx(td, "font-semibold tabular-nums")}>
                    <button type="button" className="underline-offset-2 hover:underline" onClick={() => setOpen(c.id)}>
                      {c.caseNumber}
                    </button>
                  </td>
                  <td className={td}>
                    <span className="block font-semibold">{c.subject}</span>
                    <span className="text-sm text-granite">
                      {t(`ops.caseKinds.${c.kind}`)}
                      {c.attachments > 0 && (
                        <>
                          {" · "}
                          <Paperclip className="inline size-3.5" aria-hidden /> {c.attachments}
                        </>
                      )}
                    </span>
                  </td>
                  <td className={td}>{c.applicantName || c.contractorName || "–"}</td>
                  <td className={td}>
                    <Plaque identifier={c.plotIdentifier} />
                  </td>
                  <td className={cx(td, "tabular-nums")}>{fmtDate(c.receivedOn)}</td>
                  <td className={cx(td, "tabular-nums", late && "font-semibold text-warn")}>{c.deadline ? fmtDate(c.deadline) : "–"}</td>
                  <td className={td}>{t(`ops.caseStatus.${c.status}`)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      {creating && <NewCaseDialog cemeteryId={cemeteryId} onClose={(id) => (setCreating(false), id && setOpen(id))} />}
      {open && <CaseDialog id={open} onClose={() => setOpen(null)} />}
    </Panel>
  );
}

function NewCaseDialog({ cemeteryId, onClose }: { cemeteryId: string; onClose: (id?: string) => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const contractors = useQuery({ queryKey: ["contractors"], queryFn: () => api<Contractor[]>("contractors.list") });
  const [v, setV] = useState({ kind: "monument_permit", subject: "", applicantName: "", contractorId: "", receivedOn: todayIso(), deadline: "", plotId: null as string | null });
  const [withPlot, setWithPlot] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const set = (k: keyof typeof v) => (x: string) => setV({ ...v, [k]: x });
  return (
    <Dialog
      open
      wide
      onClose={() => onClose()}
      title={t("ops.newCase")}
      footer={
        <>
          <Button onClick={() => onClose()}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              setError(null);
              try {
                const c = await api<CaseRow>("cases.create", {
                  case: { cemeteryId, ...v, contractorId: v.contractorId || null, deadline: v.deadline || null, plotId: withPlot ? v.plotId : null },
                });
                await invalidate();
                onClose(c.id);
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("ops.fileCase")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        <Field label={t("ops.caseKind")}>
          <Select value={v.kind} onChange={(e) => set("kind")(e.target.value)} name="kind">
            {CASE_KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`ops.caseKinds.${k}`)}
              </option>
            ))}
          </Select>
        </Field>
        <TextField label={t("ops.received")} type="date" value={v.receivedOn} onChange={set("receivedOn")} required />
        <TextField className="md:col-span-2" label={t("ops.subject")} value={v.subject} onChange={set("subject")} required inputProps={{ name: "subject" }} />
        <TextField label={t("ops.applicant")} value={v.applicantName} onChange={set("applicantName")} inputProps={{ name: "applicant" }} />
        <Field label={t("ops.contractor")}>
          <Select value={v.contractorId} onChange={(e) => set("contractorId")(e.target.value)}>
            <option value="">–</option>
            {(contractors.data ?? []).map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </Select>
        </Field>
        <TextField label={t("ops.deadline")} type="date" value={v.deadline} onChange={set("deadline")} hint={t("ops.deadlineHint")} />
        <div className="md:col-span-2 flex flex-col gap-3">
          <Checkbox label={t("ops.forPlot")} checked={withPlot} onChange={setWithPlot} />
          {withPlot && <PlotPicker cemeteryId={cemeteryId} value={v.plotId} onChange={(id) => setV({ ...v, plotId: id })} />}
        </div>
      </div>
      <p className="mt-4 text-sm text-granite">{t("ops.numberHint")}</p>
      <div className="mt-4">
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

function CaseDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const q = useQuery({ queryKey: ["case", id], queryFn: () => api<{ case: CaseRow; attachments: Attachment[]; history: HistoryRow[] }>("cases.get", { id }) });
  const c = q.data?.case;
  const [v, setV] = useState<{ status: string; decision: string; deadline: string } | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const form = v ?? (c ? { status: c.status, decision: c.decision, deadline: c.deadline ?? "" } : null);
  const closed = c?.status === "closed";
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={c ? `${c.caseNumber} · ${c.subject}` : t("ops.case")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.close")}</Button>
          {c && <PrintButton doc="case_document" args={{ caseId: id }} label={t("ops.issueDocument")} onDone={() => void invalidate()} />}
          {!closed && form && (
            <Button
              variant="primary"
              busy={busy}
              onClick={async () => {
                setBusy(true);
                setError(null);
                try {
                  await api("cases.update", { id, update: form });
                  await invalidate();
                  setV(null);
                } catch (e) {
                  setError(e);
                } finally {
                  setBusy(false);
                }
              }}
            >
              {t("common.save")}
            </Button>
          )}
        </>
      }
    >
      {!c || !form ? (
        <Spinner />
      ) : (
        <div className="grid gap-6 lg:grid-cols-[1fr_18rem]">
          <div className="flex flex-col gap-4">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
              <dt className="text-granite">{t("ops.caseKind")}</dt>
              <dd>{t(`ops.caseKinds.${c.kind}`)}</dd>
              <dt className="text-granite">{t("ops.applicant")}</dt>
              <dd>{c.applicantName || c.contractorName || "–"}</dd>
              <dt className="text-granite">{t("ops.received")}</dt>
              <dd className="tabular-nums">{fmtDate(c.receivedOn)}</dd>
              {c.plotIdentifier && (
                <>
                  <dt className="text-granite">{t("ops.plot")}</dt>
                  <dd>
                    <Plaque identifier={c.plotIdentifier} />
                  </dd>
                </>
              )}
            </dl>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t("common.status")}>
                <Select value={form.status} disabled={closed} onChange={(e) => setV({ ...form, status: e.target.value })} name="caseStatus">
                  {CASE_STATUSES.map((s) => (
                    <option key={s} value={s}>
                      {t(`ops.caseStatus.${s}`)}
                    </option>
                  ))}
                </Select>
              </Field>
              <TextField label={t("ops.deadline")} type="date" value={form.deadline} onChange={(x) => setV({ ...form, deadline: x })} />
            </div>
            <p className="text-sm text-granite">{t("ops.documentHint")}</p>
            <Field label={t("ops.decision")} hint={t("ops.decisionHint")}>
              <Textarea value={form.decision} disabled={closed} onChange={(e) => setV({ ...form, decision: e.target.value })} rows={4} name="decision" />
            </Field>
            <ErrorAlert error={error} />
          </div>
          <div className="flex flex-col gap-4">
            <section>
              <h3 className="mb-2 font-semibold">{t("ops.attachments")}</h3>
              <ul className="flex flex-col gap-1">
                {q.data!.attachments.map((a) => (
                  <li key={a.id}>
                    <button
                      type="button"
                      className="text-left underline-offset-2 hover:underline"
                      onClick={async () => openGenerated(await api<GeneratedFile>("attachments.get", { id: a.id }))}
                    >
                      <Paperclip className="mr-1 inline size-4" aria-hidden />
                      {a.fileName}
                    </button>{" "}
                    <span className="text-sm text-granite">{fmtSize(a.size)}</span>
                  </li>
                ))}
              </ul>
              {!closed && (
                <label className="mt-2 inline-flex min-h-11 cursor-pointer items-center gap-2 rounded-md border border-seam px-3 font-semibold hover:bg-lichen/40">
                  <Plus className="size-4" aria-hidden />
                  {t("ops.addAttachment")}
                  <input
                    type="file"
                    className="sr-only"
                    onChange={async (e) => {
                      const f = e.target.files?.[0];
                      e.target.value = "";
                      if (!f) return;
                      setError(null);
                      try {
                        await uploadFile("case", id, f);
                        await invalidate();
                      } catch (err) {
                        setError(err);
                      }
                    }}
                  />
                </label>
              )}
            </section>
            <section>
              <h3 className="mb-2 font-semibold">{t("ops.history")}</h3>
              <ol className="flex flex-col gap-2 text-sm">
                {q.data!.history.map((h, i) => (
                  <li key={i} className="border-l-2 border-seam pl-3">
                    <span className="block tabular-nums text-granite">
                      {fmtDateTime(h.ts)} · {h.userName}
                    </span>
                    {t(`ops.historyActions.${h.action}`, { defaultValue: h.action })}
                  </li>
                ))}
              </ol>
            </section>
          </div>
        </div>
      )}
    </Dialog>
  );
}

// ---------------------------------------------------------------- contractors (FR-VAL-01/02)

function ContractorsTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["contractors"], queryFn: () => api<Contractor[]>("contractors.list") });
  const from = addDays(todayIso(), -90);
  const notices = useQuery({ queryKey: ["workNotices", cemeteryId, from], queryFn: () => api<WorkNotice[]>("workNotices.list", { cemeteryId, from, to: addDays(todayIso(), 365) }) });
  const [editing, setEditing] = useState<Partial<Contractor> | null>(null);
  const [notice, setNotice] = useState(false);
  return (
    <div className="flex flex-col gap-6">
      <Panel
        title={t("ops.contractors")}
        actions={
          <Button variant="primary" icon={<Plus className="size-5" aria-hidden />} onClick={() => setEditing({ kind: "funeral" })}>
            {t("ops.newContractor")}
          </Button>
        }
      >
        {!q.data ? (
          <Spinner />
        ) : q.data.length === 0 ? (
          <Empty>{t("ops.noContractors")}</Empty>
        ) : (
          <table className="w-full min-w-[720px]" data-testid="contractors">
            <thead>
              <tr className="border-b border-seam">
                <th className={th}>{t("common.name")}</th>
                <th className={th}>{t("ops.contractorKind")}</th>
                <th className={th}>{t("ops.contact")}</th>
                <th className={th}>{t("ops.permit")}</th>
                <th className={cx(th, "text-right")}>{t("ops.noticesCount")}</th>
              </tr>
            </thead>
            <tbody>
              {q.data.map((c) => (
                <tr key={c.id} className="cursor-pointer border-b border-seam last:border-0 hover:bg-lichen/30" onClick={() => setEditing(c)}>
                  <td className={cx(td, "font-semibold")}>{c.name}</td>
                  <td className={td}>{t(`ops.contractorKinds.${c.kind}`)}</td>
                  <td className={td}>{[c.contactName, c.phone, c.email].filter(Boolean).join(" · ") || "–"}</td>
                  <td className={cx(td, c.permitExpired && "font-semibold text-warn")}>
                    {c.permitNumber || "–"}
                    {c.permitValidUntil && ` (${c.permitExpired ? t("ops.expired") : t("ops.validUntil")} ${fmtDate(c.permitValidUntil)})`}
                  </td>
                  <td className={cx(td, "text-right tabular-nums")}>{c.notices}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
      <Panel
        title={t("ops.workNotices")}
        actions={
          <Button icon={<Plus className="size-5" aria-hidden />} onClick={() => setNotice(true)} disabled={!q.data?.length}>
            {t("ops.newNotice")}
          </Button>
        }
      >
        {(notices.data ?? []).length === 0 ? (
          <Empty>{t("ops.noNotices")}</Empty>
        ) : (
          <table className="w-full min-w-[720px]" data-testid="work-notices">
            <thead>
              <tr className="border-b border-seam">
                <th className={th}>{t("common.date")}</th>
                <th className={th}>{t("ops.contractor")}</th>
                <th className={th}>{t("ops.noticeKind")}</th>
                <th className={th}>{t("ops.plot")}</th>
                <th className={cx(th, "text-right")}>{t("ops.contribution")}</th>
              </tr>
            </thead>
            <tbody>
              {notices.data!.map((n) => (
                <tr key={n.id} className="border-b border-seam last:border-0">
                  <td className={cx(td, "tabular-nums")}>{fmtDate(n.workDate)}</td>
                  <td className={cx(td, "font-semibold")}>{n.contractorName}</td>
                  <td className={td}>
                    {t(`ops.noticeKinds.${n.kind}`)}
                    {n.description && <span className="block text-sm text-granite">{n.description}</span>}
                  </td>
                  <td className={td}>
                    <Plaque identifier={n.plotIdentifier} />
                  </td>
                  <td className={cx(td, "text-right tabular-nums")}>{n.chargeGrossHuf != null ? fmtHuf(n.chargeGrossHuf) : "–"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
      {editing && <ContractorDialog cemeteryId={cemeteryId} contractor={editing} onClose={() => setEditing(null)} />}
      {notice && <NoticeDialog cemeteryId={cemeteryId} contractors={q.data ?? []} onClose={() => setNotice(false)} />}
    </div>
  );
}

function ContractorDialog({ cemeteryId, contractor, onClose }: { cemeteryId: string; contractor: Partial<Contractor>; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [v, setV] = useState({
    name: contractor.name ?? "",
    kind: contractor.kind ?? "funeral",
    taxNumber: contractor.taxNumber ?? "",
    postcode: contractor.postcode ?? "",
    city: contractor.city ?? "",
    address: contractor.address ?? "",
    contactName: contractor.contactName ?? "",
    phone: contractor.phone ?? "",
    email: contractor.email ?? "",
    permitNumber: contractor.permitNumber ?? "",
    permitValidUntil: contractor.permitValidUntil ?? "",
    notes: contractor.notes ?? "",
  });
  const [error, setError] = useState<unknown>(null);
  const set = (k: keyof typeof v) => (x: string) => setV({ ...v, [k]: x });
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={contractor.id ? contractor.name! : t("ops.newContractor")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              setError(null);
              try {
                await api("contractors.save", { contractor: { id: contractor.id ?? null, ...v, permitValidUntil: v.permitValidUntil || null } });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-3">
        <TextField className="md:col-span-2" label={t("common.name")} value={v.name} onChange={set("name")} required inputProps={{ name: "contractorName" }} />
        <Field label={t("ops.contractorKind")}>
          <Select value={v.kind} onChange={(e) => set("kind")(e.target.value)}>
            {(["funeral", "stonemason", "gardener", "other"] as const).map((k) => (
              <option key={k} value={k}>
                {t(`ops.contractorKinds.${k}`)}
              </option>
            ))}
          </Select>
        </Field>
        <TextField label={t("ops.taxNumber")} value={v.taxNumber} onChange={set("taxNumber")} />
        <TextField label={t("ops.postcode")} value={v.postcode} onChange={set("postcode")} />
        <TextField label={t("ops.city")} value={v.city} onChange={set("city")} />
        <TextField className="md:col-span-3" label={t("ops.address")} value={v.address} onChange={set("address")} />
        <TextField label={t("ops.contactName")} value={v.contactName} onChange={set("contactName")} />
        <TextField label={t("ops.phone")} type="tel" value={v.phone} onChange={set("phone")} />
        <TextField label={t("ops.email")} type="email" value={v.email} onChange={set("email")} />
        <TextField label={t("ops.permitNumber")} value={v.permitNumber} onChange={set("permitNumber")} />
        <TextField label={t("ops.permitValidUntil")} type="date" value={v.permitValidUntil} onChange={set("permitValidUntil")} />
        <Field label={t("common.notes")} className="md:col-span-3">
          <Textarea value={v.notes} onChange={(e) => set("notes")(e.target.value)} rows={2} />
        </Field>
      </div>
      <div className="mt-4">
        <ErrorAlert error={error} />
      </div>
      {contractor.id && <ContractorAccount cemeteryId={cemeteryId} contractorId={contractor.id} />}
    </Dialog>
  );
}

interface ContractorAccountData {
  months: { month: string; notices: number; grossHuf: number }[];
  openCharges: (Charge & { cemeteryId: string })[];
  openHuf: number;
  invoicedHuf: number;
  unpaidHuf: number;
}

/** FR-VAL-03: turnover, debt and a summary invoice of the open contributions. */
function ContractorAccount({ cemeteryId, contractorId }: { cemeteryId: string; contractorId: string }) {
  const { t } = useTranslation();
  const { can } = useApp();
  const q = useQuery({ queryKey: ["contractorAccount", contractorId], queryFn: () => api<ContractorAccountData>("contractors.account", { id: contractorId }) });
  const [invoicing, setInvoicing] = useState(false);
  const a = q.data;
  if (!a) return null;
  const here = a.openCharges.filter((c) => c.cemeteryId === cemeteryId);
  return (
    <section className="mt-6 border-t border-seam pt-5" data-testid="contractor-account">
      <h3 className="mb-3 text-lg">{t("ops.account")}</h3>
      <dl className="mb-4 grid gap-4 sm:grid-cols-3">
        <div>
          <dt className="text-sm text-granite">{t("ops.openItems")}</dt>
          <dd className="text-xl font-semibold tabular-nums">{fmtHuf(a.openHuf)}</dd>
        </div>
        <div>
          <dt className="text-sm text-granite">{t("ops.invoiced")}</dt>
          <dd className="text-xl font-semibold tabular-nums">{fmtHuf(a.invoicedHuf)}</dd>
        </div>
        <div>
          <dt className="text-sm text-granite">{t("ops.unpaid")}</dt>
          <dd className={cx("text-xl font-semibold tabular-nums", a.unpaidHuf > 0 && "text-warn")}>{fmtHuf(a.unpaidHuf)}</dd>
        </div>
      </dl>
      {a.months.length > 0 && (
        <table className="mb-4 w-full max-w-md">
          <thead>
            <tr className="border-b border-seam">
              <th className={th}>{t("ops.month")}</th>
              <th className={cx(th, "text-right")}>{t("ops.noticesCount")}</th>
              <th className={cx(th, "text-right")}>{t("ops.contribution")}</th>
            </tr>
          </thead>
          <tbody>
            {a.months.map((m) => (
              <tr key={m.month} className="border-b border-seam last:border-0">
                <td className={cx(td, "tabular-nums")}>{m.month.replace("-", ". ")}.</td>
                <td className={cx(td, "text-right tabular-nums")}>{m.notices}</td>
                <td className={cx(td, "text-right tabular-nums")}>{fmtHuf(m.grossHuf)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {can("record_payments") && here.length > 0 && (
        <Button variant="primary" onClick={() => setInvoicing(true)}>
          {t("ops.summaryInvoice", { count: here.length })}
        </Button>
      )}
      {invoicing && <InvoiceDialog cemeteryId={cemeteryId} charges={here} onClose={() => setInvoicing(false)} />}
    </section>
  );
}

function NoticeDialog({ cemeteryId, contractors, onClose }: { cemeteryId: string; contractors: Contractor[]; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const valid = contractors.filter((c) => !c.permitExpired);
  const [v, setV] = useState({ contractorId: valid[0]?.id ?? "", kind: "monument", workDate: todayIso(), description: "", plotId: null as string | null, charge: true });
  const [error, setError] = useState<unknown>(null);
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={t("ops.newNotice")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              setError(null);
              try {
                await api("workNotices.add", { notice: { cemeteryId, ...v } });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-3">
        <Field label={t("ops.contractor")}>
          <Select value={v.contractorId} onChange={(e) => setV({ ...v, contractorId: e.target.value })} name="contractor">
            {valid.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("ops.noticeKind")}>
          <Select value={v.kind} onChange={(e) => setV({ ...v, kind: e.target.value })}>
            {["burial", "monument", "renovation", "demolition", "other"].map((k) => (
              <option key={k} value={k}>
                {t(`ops.noticeKinds.${k}`)}
              </option>
            ))}
          </Select>
        </Field>
        <TextField label={t("ops.workDate")} type="date" value={v.workDate} onChange={(x) => setV({ ...v, workDate: x })} required />
        <TextField className="md:col-span-3" label={t("ops.description")} value={v.description} onChange={(x) => setV({ ...v, description: x })} />
        <div className="md:col-span-3">
          <PlotPicker cemeteryId={cemeteryId} value={v.plotId} onChange={(id) => setV({ ...v, plotId: id })} />
        </div>
        <div className="md:col-span-3">
          <Checkbox label={t("ops.chargeContribution")} hint={t("ops.chargeContributionHint")} checked={v.charge} onChange={(x) => setV({ ...v, charge: x })} />
        </div>
      </div>
      <div className="mt-4">
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------- inspection (FR-UZE-02/03)

function InspectionTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const q = useQuery({ queryKey: ["conditions", cemeteryId], queryFn: () => api<ConditionRow[]>("conditions.list", { cemeteryId, problemsOnly: true }) });
  const [plotId, setPlotId] = useState<string | null>(null);
  const [condition, setCondition] = useState<ConditionRow["condition"]>("neglected");
  const [note, setNote] = useState("");
  const [openCase, setOpenCase] = useState(true);
  const [photo, setPhoto] = useState<File | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const counts = useMemo(() => {
    const m: Record<string, number> = {};
    for (const r of q.data ?? []) m[r.condition] = (m[r.condition] ?? 0) + 1;
    return m;
  }, [q.data]);
  return (
    <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <Panel title={t("ops.recordCondition")}>
        <div className="flex flex-col gap-4">
          <PlotPicker cemeteryId={cemeteryId} value={plotId} onChange={(id) => (setPlotId(id), setSaved(null))} />
          <RadioGroup
            label={t("ops.conditionLabel")}
            value={condition}
            onChange={setCondition}
            options={(["ok", "neglected", "dangerous", "to_liquidate"] as const).map((c) => ({ value: c, label: t(`ops.conditions.${c}`) }))}
          />
          <Field label={t("common.notes")}>
            <Textarea value={note} onChange={(e) => setNote(e.target.value)} rows={2} name="conditionNote" />
          </Field>
          <Field label={t("ops.photo")} hint={t("ops.photoHint")}>
            <input type="file" accept="image/*" capture="environment" key={saved ?? "new"} onChange={(e) => setPhoto(e.target.files?.[0] ?? null)} />
          </Field>
          {(condition === "neglected" || condition === "dangerous") && (
            <Checkbox label={t("ops.openNeglectCase")} hint={t("ops.openNeglectCaseHint")} checked={openCase} onChange={setOpenCase} />
          )}
          <ErrorAlert error={error} />
          {saved && <Alert tone="ok">{saved}</Alert>}
          <div>
            <Button
              variant="primary"
              disabled={!plotId}
              onClick={async () => {
                setError(null);
                try {
                  if (photo && plotId) await uploadFile("plot", plotId, photo);
                  const r = await api<{ caseId: string | null }>("conditions.record", { condition: { plotId, condition, note, openCase } });
                  setPhoto(null);
                  setSaved(r.caseId ? t("ops.conditionSavedCase") : t("ops.conditionSaved"));
                  setNote("");
                  await invalidate();
                } catch (e) {
                  setError(e);
                }
              }}
            >
              {t("ops.saveCondition")}
            </Button>
          </div>
        </div>
      </Panel>
      <Panel title={t("ops.problemPlots")}>
        {Object.keys(counts).length > 0 && (
          <p className="mb-3 text-granite">
            {Object.entries(counts)
              .map(([k, n]) => `${t(`ops.conditions.${k}`)}: ${n}`)
              .join(" · ")}
          </p>
        )}
        {(q.data ?? []).length === 0 ? (
          <Empty>{t("ops.noProblems")}</Empty>
        ) : (
          <table className="w-full min-w-[520px]" data-testid="conditions">
            <thead>
              <tr className="border-b border-seam">
                <th className={th}>{t("ops.plot")}</th>
                <th className={th}>{t("ops.conditionLabel")}</th>
                <th className={th}>{t("common.date")}</th>
                <th className={th}>{t("ops.caseNumber")}</th>
              </tr>
            </thead>
            <tbody>
              {q.data!.map((r) => (
                <tr key={r.id} className="border-b border-seam last:border-0">
                  <td className={td}>
                    <Plaque identifier={r.plotIdentifier} />
                  </td>
                  <td className={cx(td, r.condition === "dangerous" && "font-semibold text-alert")}>
                    {t(`ops.conditions.${r.condition}`)}
                    {r.note && <span className="block text-sm text-granite">{r.note}</span>}
                  </td>
                  <td className={cx(td, "tabular-nums")}>{fmtDate(r.observedOn)}</td>
                  <td className={cx(td, "tabular-nums")}>{r.caseNumber ?? "–"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
    </div>
  );
}
