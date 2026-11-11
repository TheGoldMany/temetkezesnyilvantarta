import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Banknote, FileText, Link2, Plus, Upload } from "lucide-react";
import { api } from "../../lib/api";
import { useApp } from "../../app/context";
import { useCurrentCemetery, useInvalidate } from "../../lib/queries";
import { fmtDate, fmtHuf, todayIso } from "../../lib/format";
import { Alert, Button, Checkbox, DateInput, Dialog, Empty, ErrorAlert, Field, PageHeader, Panel, Select, Spinner, Tabs, TextField, cx } from "../../components/ui";
import { openGenerated, type GeneratedFile } from "../prints/PrintButton";
import { StockTab } from "./Stock";
import type { FeeRate } from "../../lib/types";

export interface Charge {
  id: string;
  payerName: string;
  personId: string | null;
  description: string;
  quantity: number;
  unit: string;
  unitNetHuf: number;
  vatRate: string;
  netHuf: number;
  grossHuf: number;
  status: string;
  createdAt: string;
}
export interface Invoice {
  id: string;
  kind: "invoice" | "proforma" | "storno";
  provider: "szamlazz" | "asp";
  number: string;
  buyerName: string;
  buyerAddress: string;
  issueDate: string;
  dueDate: string;
  paymentMethod: "cash" | "transfer" | "card";
  netHuf: number;
  grossHuf: number;
  paidHuf: number;
  status: "pending_asp" | "issued" | "paid" | "partially_paid" | "storno";
  hasPdf: boolean;
  daysOverdue: number | null;
  reminders: number;
  buyerEmail: string;
}
interface CashEntry {
  id: string;
  kind: "in" | "out";
  docNumber: string;
  amountHuf: number;
  partnerName: string;
  description: string;
  voidedAt: string | null;
}
interface CashDay {
  date: string;
  openingHuf: number;
  inHuf: number;
  outHuf: number;
  closingHuf: number;
  entries: CashEntry[];
  closed: { countedHuf: number; note: string; closedBy: string | null } | null;
}
interface BankTx {
  id: string;
  bookedOn: string;
  amountHuf: number;
  partner: string;
  reference: string;
  status: string;
  suggestedInvoiceId: string | null;
  suggestedInvoiceNumber: string | null;
}
interface FinanceStatus {
  provider: "szamlazz" | "asp";
  agentKeySet: boolean;
  dueDays: number;
  defaultVat: string;
  vatCodes: string[];
}

type Tab = "charges" | "invoices" | "cash" | "stock" | "bank" | "receivables" | "report" | "setup";

/** W7 – Pénzügy: items, invoices, payments, cash desk, bank, receivables, reports. */
export function FinanceView({ tab: initial }: { tab?: string } = {}) {
  const { t } = useTranslation();
  const { can } = useApp();
  const { current } = useCurrentCemetery();
  const full = can("record_payments");
  const tabs: Tab[] = full ? ["charges", "invoices", "cash", "stock", "bank", "receivables", "report", ...(can("manage_settings") ? (["setup"] as Tab[]) : [])] : ["cash", "stock"];
  const [tab, setTab] = useState<Tab>(tabs.includes(initial as Tab) ? (initial as Tab) : tabs[0]);
  if (!current) return <Spinner />;
  return (
    <>
      <PageHeader title={t("finance.title")} subtitle={current.name} />
      <Tabs value={tab} onChange={setTab} items={tabs.map((x) => ({ value: x, label: t(`finance.tabs.${x}`) }))} />
      <div className="mt-6">
        {tab === "charges" && <ChargesTab cemeteryId={current.id} />}
        {tab === "invoices" && <InvoicesTab cemeteryId={current.id} />}
        {tab === "cash" && <CashTab cemeteryId={current.id} />}
        {tab === "stock" && <StockTab cemeteryId={current.id} />}
        {tab === "bank" && <BankTab cemeteryId={current.id} />}
        {tab === "receivables" && <ReceivablesTab cemeteryId={current.id} />}
        {tab === "report" && <ReportTab cemeteryId={current.id} />}
        {tab === "setup" && <SetupTab />}
      </div>
    </>
  );
}

const th = "py-2 pr-4 text-left text-sm font-medium text-granite";
const td = "py-2.5 pr-4";

// ---------------------------------------------------------------- items

function ChargesTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["charges", cemeteryId], queryFn: () => api<Charge[]>("charges.list", { cemeteryId, status: "open" }) });
  const [adding, setAdding] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [invoicing, setInvoicing] = useState(false);
  const rows = q.data ?? [];
  const chosen = rows.filter((r) => selected.includes(r.id));
  const samePayer = new Set(chosen.map((c) => c.payerName)).size <= 1;
  return (
    <div className="flex flex-col gap-6">
      <Panel
        title={t("finance.openCharges")}
        actions={
          <>
            <Button icon={<Plus className="size-4" aria-hidden />} onClick={() => setAdding(true)}>
              {t("finance.newCharge")}
            </Button>
            <Button variant="primary" icon={<FileText className="size-4" aria-hidden />} disabled={!chosen.length || !samePayer} onClick={() => setInvoicing(true)}>
              {t("finance.invoiceSelected", { count: chosen.length })}
            </Button>
          </>
        }
      >
        {!samePayer && <Alert tone="warn">{t("finance.onePayer")}</Alert>}
        {rows.length === 0 ? (
          <Empty>{t("finance.noCharges")}</Empty>
        ) : (
          <table className="w-full min-w-[720px]" data-testid="charges">
            <thead>
              <tr className="border-b border-seam">
                <th className="w-10" />
                <th className={th}>{t("finance.payer")}</th>
                <th className={th}>{t("finance.item")}</th>
                <th className={cx(th, "text-right")}>{t("finance.net")}</th>
                <th className={th}>{t("finance.vat")}</th>
                <th className={cx(th, "text-right")}>{t("finance.gross")}</th>
                <th className={th}>{t("common.date")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <tr key={c.id} className="border-b border-seam last:border-0">
                  <td className={td}>
                    <input
                      type="checkbox"
                      className="size-5 accent-[var(--c-moss)]"
                      aria-label={c.description}
                      checked={selected.includes(c.id)}
                      onChange={(e) => setSelected(e.target.checked ? [...selected, c.id] : selected.filter((x) => x !== c.id))}
                    />
                  </td>
                  <td className={cx(td, "font-semibold")}>{c.payerName}</td>
                  <td className={td}>
                    {c.description}
                    {c.quantity > 1 && <span className="text-granite"> ({c.quantity} {c.unit})</span>}
                  </td>
                  <td className={cx(td, "text-right tabular-nums")}>{fmtHuf(c.netHuf)}</td>
                  <td className={td}>{vatLabel(c.vatRate)}</td>
                  <td className={cx(td, "text-right font-semibold tabular-nums")}>{fmtHuf(c.grossHuf)}</td>
                  <td className={cx(td, "tabular-nums")}>{fmtDate(c.createdAt)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
      {adding && <ChargeDialog cemeteryId={cemeteryId} onClose={() => setAdding(false)} />}
      {invoicing && (
        <InvoiceDialog
          cemeteryId={cemeteryId}
          charges={chosen}
          onClose={(done) => {
            setInvoicing(false);
            if (done) setSelected([]);
          }}
        />
      )}
    </div>
  );
}

const vatLabel = (v: string) => (/^\d+$/.test(v) ? `${v}%` : v);

function ChargeDialog({ cemeteryId, onClose }: { cemeteryId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const fees = useQuery({ queryKey: ["fees", cemeteryId], queryFn: () => api<FeeRate[]>("feeRates.list", { cemeteryId }) });
  const active = (fees.data ?? []).filter((f) => !f.voidedAt && (!f.validTo || f.validTo >= todayIso()) && f.validFrom <= todayIso());
  const [v, setV] = useState({ feeRateId: "", payerName: "", quantity: "1", price: "", reason: "", description: "", vatRate: "27" });
  const [error, setError] = useState<unknown>(null);
  const rate = active.find((f) => f.id === v.feeRateId);
  const priceChanged = rate && v.price !== "" && Number(v.price) !== rate.amountHuf;
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("finance.newCharge")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              setError(null);
              try {
                await api("charges.add", {
                  charge: {
                    cemeteryId,
                    payerName: v.payerName,
                    feeRateId: v.feeRateId || null,
                    quantity: Number(v.quantity) || 1,
                    unitNetHuf: v.price === "" ? null : Number(v.price),
                    description: v.description || null,
                    vatRate: rate ? null : v.vatRate,
                    overrideReason: priceChanged ? v.reason : null,
                  },
                });
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
        <Field label={t("finance.feeItem")} className="md:col-span-2">
          <Select value={v.feeRateId} onChange={(e) => setV({ ...v, feeRateId: e.target.value, price: "" })} name="feeRate">
            <option value="">{t("finance.customItem")}</option>
            {active.map((f) => (
              <option key={f.id} value={f.id}>
                {(f.name || t(`settings.feeKinds.${f.feeKind}`, { defaultValue: f.feeKind })) + ` – ${fmtHuf(f.amountHuf)} + ${vatLabel(f.vatRate)}`}
              </option>
            ))}
          </Select>
        </Field>
        <TextField className="md:col-span-2" label={t("finance.payer")} value={v.payerName} onChange={(x) => setV({ ...v, payerName: x })} required inputProps={{ name: "payerName" }} />
        {!rate && (
          <>
            <TextField className="md:col-span-2" label={t("finance.item")} value={v.description} onChange={(x) => setV({ ...v, description: x })} required inputProps={{ name: "description" }} />
            <Field label={t("finance.vat")}>
              <Select value={v.vatRate} onChange={(e) => setV({ ...v, vatRate: e.target.value })}>
                {["27", "18", "5", "0", "AAM", "TAM"].map((x) => (
                  <option key={x} value={x}>
                    {vatLabel(x)}
                  </option>
                ))}
              </Select>
            </Field>
          </>
        )}
        <TextField label={t("finance.quantity")} type="number" value={v.quantity} onChange={(x) => setV({ ...v, quantity: x })} />
        <TextField label={t("finance.unitNet")} type="number" value={v.price} onChange={(x) => setV({ ...v, price: x })} hint={rate ? t("finance.fromDecree", { amount: fmtHuf(rate.amountHuf) }) : undefined} inputProps={{ name: "price" }} />
        {priceChanged && <TextField className="md:col-span-2" label={t("common.reason")} value={v.reason} onChange={(x) => setV({ ...v, reason: x })} required />}
      </div>
      <div className="mt-4">
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

export function InvoiceDialog({ cemeteryId, charges, onClose }: { cemeteryId: string; charges: Charge[]; onClose: (done: boolean) => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const status = useQuery({ queryKey: ["finance-status"], queryFn: () => api<FinanceStatus>("finance.status") });
  const [v, setV] = useState({ buyerName: charges[0]?.payerName ?? "", postcode: "", city: "", address: "", taxNumber: "", email: "", method: "transfer", proforma: false });
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const total = charges.reduce((s, c) => s + c.grossHuf, 0);
  const set = (k: keyof typeof v) => (x: string) => setV({ ...v, [k]: x });
  return (
    <Dialog
      open
      wide
      onClose={() => onClose(false)}
      title={t("finance.invoiceTitle")}
      footer={
        <>
          <Button onClick={() => onClose(false)}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            busy={busy}
            onClick={async () => {
              setBusy(true);
              setError(null);
              try {
                await api("invoices.create", {
                  invoice: {
                    cemeteryId,
                    chargeIds: charges.map((c) => c.id),
                    personId: charges[0]?.personId ?? null,
                    buyerName: v.buyerName,
                    buyerPostcode: v.postcode,
                    buyerCity: v.city,
                    buyerAddress: v.address,
                    buyerTaxNumber: v.taxNumber,
                    buyerEmail: v.email,
                    paymentMethod: v.method,
                    proforma: v.proforma,
                  },
                });
                await invalidate();
                onClose(true);
              } catch (e) {
                setError(e);
              } finally {
                setBusy(false);
              }
            }}
          >
            {status.data?.provider === "asp" ? t("finance.prepareAsp") : v.proforma ? t("finance.issueProforma") : t("finance.issueInvoice")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {status.data?.provider === "asp" ? (
          <Alert>{t("finance.aspHint")}</Alert>
        ) : (
          !status.data?.agentKeySet && <Alert tone="warn">{t("finance.testMode")}</Alert>
        )}
        <p>
          {t("finance.itemsTotal", { count: charges.length })} <b className="tabular-nums">{fmtHuf(total)}</b>
        </p>
        <div className="grid gap-4 md:grid-cols-6">
          <TextField className="md:col-span-4" label={t("billing.name")} value={v.buyerName} onChange={set("buyerName")} required inputProps={{ name: "buyerName" }} />
          <TextField className="md:col-span-2" label={t("billing.taxNumber")} value={v.taxNumber} onChange={set("taxNumber")} />
          <TextField className="md:col-span-1" label={t("billing.postcode")} value={v.postcode} onChange={set("postcode")} required inputProps={{ name: "buyerPostcode" }} />
          <TextField className="md:col-span-2" label={t("billing.city")} value={v.city} onChange={set("city")} required inputProps={{ name: "buyerCity" }} />
          <TextField className="md:col-span-3" label={t("billing.address")} value={v.address} onChange={set("address")} required inputProps={{ name: "buyerAddress" }} />
          <TextField className="md:col-span-3" label={t("fields.email")} type="email" value={v.email} onChange={set("email")} hint={t("finance.emailHint")} />
          <Field label={t("billing.paymentMethod")} className="md:col-span-3">
            <Select value={v.method} onChange={(e) => setV({ ...v, method: e.target.value })} name="paymentMethod">
              <option value="transfer">{t("finance.method.transfer")}</option>
              <option value="cash">{t("finance.method.cash")}</option>
              <option value="card">{t("finance.method.card")}</option>
            </Select>
          </Field>
        </div>
        {status.data?.provider !== "asp" && v.method !== "cash" && <Checkbox label={t("finance.proforma")} checked={v.proforma} onChange={(b) => setV({ ...v, proforma: b })} />}
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------- invoices

function InvoicesTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [filter, setFilter] = useState<"all" | "open" | "overdue" | "pending_asp">("all");
  const q = useQuery({
    queryKey: ["invoices", cemeteryId, filter],
    queryFn: () => api<Invoice[]>("invoices.list", { cemeteryId, status: filter === "pending_asp" ? "pending_asp" : null, overdueOnly: filter === "overdue" }),
  });
  const rows = (q.data ?? []).filter((i) => filter !== "open" || ["issued", "partially_paid"].includes(i.status));
  const [paying, setPaying] = useState<Invoice | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [link, setLink] = useState<{ number: string; url: string } | null>(null);
  const act = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await invalidate();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Panel
      actions={
        <Select aria-label={t("finance.filter")} value={filter} onChange={(e) => setFilter(e.target.value as typeof filter)}>
          {(["all", "open", "overdue", "pending_asp"] as const).map((f) => (
            <option key={f} value={f}>
              {t(`finance.filters.${f}`)}
            </option>
          ))}
        </Select>
      }
    >
      <ErrorAlert error={error} />
      {link && (
        <div className="mb-4">
          <Alert tone="ok">
            <span className="block">{t("finance.payLinkReady", { number: link.number })}</span>
            <a className="break-all font-semibold text-moss underline underline-offset-3" href={link.url} target="_blank" rel="noreferrer" data-testid="pay-link">
              {link.url}
            </a>
          </Alert>
        </div>
      )}
      {rows.length === 0 ? (
        <Empty>{t("finance.noInvoices")}</Empty>
      ) : (
        <table className="w-full min-w-[860px]" data-testid="invoices-table">
          <thead>
            <tr className="border-b border-seam">
              <th className={th}>{t("billing.number")}</th>
              <th className={th}>{t("finance.buyer")}</th>
              <th className={th}>{t("finance.issued")}</th>
              <th className={cx(th, "text-right")}>{t("finance.gross")}</th>
              <th className={th}>{t("billing.state")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((i) => (
              <tr key={i.id} className="border-b border-seam last:border-0">
                <td className={cx(td, "font-semibold tabular-nums")}>
                  {i.number || <span className="text-granite">{t("finance.noNumber")}</span>}
                  <span className="block text-sm font-normal text-granite">{t(`finance.kind.${i.kind}`)}</span>
                </td>
                <td className={td}>
                  {i.buyerName}
                  <span className="block text-sm text-granite">{i.buyerAddress}</span>
                </td>
                <td className={cx(td, "tabular-nums")}>
                  {fmtDate(i.issueDate)}
                  <span className="block text-sm text-granite">
                    {t("finance.due")}: {fmtDate(i.dueDate)}
                  </span>
                </td>
                <td className={cx(td, "text-right tabular-nums")}>
                  <b>{fmtHuf(i.grossHuf)}</b>
                  {i.paidHuf > 0 && i.paidHuf < i.grossHuf && <span className="block text-sm text-granite">{t("finance.paidSoFar", { amount: fmtHuf(i.paidHuf) })}</span>}
                </td>
                <td className={td}>
                  <span className={cx(i.daysOverdue ? "font-semibold text-alert" : "")}>{i.daysOverdue ? t("finance.overdue", { count: i.daysOverdue }) : t(`finance.status.${i.status}`)}</span>
                  {i.reminders > 0 && <span className="block text-sm text-granite">{t("finance.reminders", { count: i.reminders })}</span>}
                </td>
                <td className={cx(td, "pr-0 text-right")}>
                  <div className="flex flex-wrap justify-end gap-2">
                    {i.hasPdf && (
                      <Button variant="quiet" onClick={() => act(async () => openGenerated(await api<GeneratedFile>("invoices.pdf", { id: i.id })))}>
                        PDF
                      </Button>
                    )}
                    {i.kind === "invoice" && ["issued", "partially_paid", "pending_asp"].includes(i.status) && (
                      <Button icon={<Banknote className="size-4" aria-hidden />} onClick={() => setPaying(i)}>
                        {t("finance.recordPayment")}
                      </Button>
                    )}
                    {i.status === "pending_asp" && (
                      <Button
                        onClick={() => {
                          const n = window.prompt(t("finance.aspNumberPrompt"));
                          if (n) void act(() => api("invoices.aspNumber", { id: i.id, number: n }));
                        }}
                      >
                        {t("finance.aspNumber")}
                      </Button>
                    )}
                    {i.kind === "invoice" && ["issued", "partially_paid"].includes(i.status) && (
                      <Button
                        variant="quiet"
                        icon={<Link2 className="size-4" aria-hidden />}
                        onClick={() =>
                          act(async () => {
                            const r = await api<{ url: string }>("invoices.payLink", { id: i.id });
                            await navigator.clipboard?.writeText(r.url).catch(() => undefined);
                            setLink({ number: i.number, url: r.url });
                          })
                        }
                      >
                        {t("finance.payLink")}
                      </Button>
                    )}
                    {i.daysOverdue && (
                      <Button variant="quiet" onClick={() => act(() => api("invoices.reminder", { id: i.id, channel: "letter" }))}>
                        {t("finance.remind")}
                      </Button>
                    )}
                    {i.daysOverdue && i.buyerEmail && (
                      <Button variant="quiet" onClick={() => act(() => api("invoices.reminder", { id: i.id, channel: "email" }))}>
                        {t("finance.remindEmail")}
                      </Button>
                    )}
                    {i.kind === "invoice" && i.status !== "storno" && i.paidHuf === 0 && (
                      <Button
                        variant="quiet"
                        onClick={() => {
                          const r = window.prompt(t("finance.stornoPrompt"));
                          if (r) void act(() => api("invoices.storno", { id: i.id, reason: r }));
                        }}
                      >
                        {t("finance.storno")}
                      </Button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {paying && <PaymentDialog cemeteryId={cemeteryId} invoice={paying} onClose={() => setPaying(null)} />}
    </Panel>
  );
}

function PaymentDialog({ cemeteryId, invoice, onClose }: { cemeteryId: string; invoice: Invoice; onClose: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [v, setV] = useState({ amount: String(invoice.grossHuf - invoice.paidHuf), method: invoice.paymentMethod, date: todayIso(), reference: "" });
  const [error, setError] = useState<unknown>(null);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("finance.recordPayment")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              try {
                await api("payments.record", { payment: { cemeteryId, invoiceId: invoice.id, amountHuf: Number(v.amount), method: v.method, paidOn: v.date, reference: v.reference } });
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
        <p className="md:col-span-2">
          {invoice.number || t("finance.noNumber")}, {invoice.buyerName}
        </p>
        <TextField label={t("finance.amount")} type="number" value={v.amount} onChange={(x) => setV({ ...v, amount: x })} inputProps={{ name: "amount" }} />
        <Field label={t("billing.paymentMethod")}>
          <Select value={v.method} onChange={(e) => setV({ ...v, method: e.target.value as Invoice["paymentMethod"] })}>
            <option value="transfer">{t("finance.method.transfer")}</option>
            <option value="cash">{t("finance.method.cash")}</option>
            <option value="card">{t("finance.method.card")}</option>
          </Select>
        </Field>
        <Field label={t("common.date")}>
          <DateInput value={v.date} onValue={(x) => setV({ ...v, date: x })} />
        </Field>
        <TextField label={t("finance.reference")} value={v.reference} onChange={(x) => setV({ ...v, reference: x })} />
        {v.method === "cash" && <p className="text-sm text-granite md:col-span-2">{t("finance.cashReceipt")}</p>}
      </div>
      <div className="mt-4">
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------- cash desk

function CashTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [date, setDate] = useState(todayIso());
  const q = useQuery({ queryKey: ["cash", cemeteryId, date], queryFn: () => api<CashDay>("cash.day", { cemeteryId, date }), enabled: /^\d{4}-\d{2}-\d{2}$/.test(date) });
  const [v, setV] = useState({ kind: "in", amount: "", partner: "", description: "" });
  const [counted, setCounted] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<unknown>(null);
  const d = q.data;
  const run = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await invalidate();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end gap-4">
        <Field label={t("finance.cashDate")}>
          <DateInput value={date} onValue={setDate} />
        </Field>
        {d && (
          <dl className="flex flex-wrap gap-x-8 gap-y-2" data-testid="cash-summary">
            {(
              [
                ["opening", d.openingHuf],
                ["in", d.inHuf],
                ["out", d.outHuf],
                ["closing", d.closingHuf],
              ] as const
            ).map(([k, n]) => (
              <div key={k}>
                <dt className="text-sm text-granite">{t(`finance.cash.${k}`)}</dt>
                <dd className="m-0 text-lg font-semibold tabular-nums">{fmtHuf(n)}</dd>
              </div>
            ))}
          </dl>
        )}
      </div>
      <ErrorAlert error={error} />
      {d && !d.closed && (
        <Panel title={t("finance.cashNew")}>
          <form
            className="grid items-end gap-4 md:grid-cols-[10rem_10rem_1fr_1fr_auto]"
            onSubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await api("cash.add", { cemeteryId, date, kind: v.kind, amountHuf: Number(v.amount), partnerName: v.partner, description: v.description });
                setV({ kind: v.kind, amount: "", partner: "", description: "" });
              });
            }}
          >
            <Field label={t("finance.cashKind")}>
              <Select value={v.kind} onChange={(e) => setV({ ...v, kind: e.target.value })} name="cashKind">
                <option value="in">{t("finance.cash.inDoc")}</option>
                <option value="out">{t("finance.cash.outDoc")}</option>
              </Select>
            </Field>
            <TextField label={t("finance.amount")} type="number" value={v.amount} onChange={(x) => setV({ ...v, amount: x })} inputProps={{ name: "cashAmount" }} />
            <TextField label={t("finance.partner")} value={v.partner} onChange={(x) => setV({ ...v, partner: x })} inputProps={{ name: "cashPartner" }} />
            <TextField label={t("finance.item")} value={v.description} onChange={(x) => setV({ ...v, description: x })} inputProps={{ name: "cashDescription" }} />
            <Button type="submit" variant="primary" disabled={!v.amount || !v.partner || !v.description}>
              {t("finance.cashBook")}
            </Button>
          </form>
        </Panel>
      )}
      <Panel title={t("finance.cashBookTitle")}>
        {!d || d.entries.length === 0 ? (
          <Empty>{t("finance.cashEmpty")}</Empty>
        ) : (
          <table className="w-full min-w-[640px]" data-testid="cash-entries">
            <tbody>
              {d.entries.map((e) => (
                <tr key={e.id} className={cx("border-b border-seam last:border-0", e.voidedAt && "text-granite line-through")}>
                  <td className={cx(td, "font-semibold tabular-nums")}>{e.docNumber}</td>
                  <td className={td}>{e.partnerName}</td>
                  <td className={td}>{e.description}</td>
                  <td className={cx(td, "pr-0 text-right font-semibold tabular-nums", e.kind === "out" && "text-alert")}>
                    {e.kind === "out" ? "−" : ""}
                    {fmtHuf(e.amountHuf)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
      {d &&
        (d.closed ? (
          <Alert tone="ok">{t("finance.cashClosed", { amount: fmtHuf(d.closed.countedHuf), by: d.closed.closedBy ?? "" })}</Alert>
        ) : (
          <Panel title={t("finance.cashClose")}>
            <div className="grid items-end gap-4 md:grid-cols-[14rem_1fr_auto]">
              <TextField label={t("finance.counted")} type="number" value={counted} onChange={setCounted} hint={t("finance.expected", { amount: fmtHuf(d.closingHuf) })} inputProps={{ name: "counted" }} />
              <TextField label={t("finance.differenceNote")} value={note} onChange={setNote} />
              <Button variant="primary" disabled={counted === ""} onClick={() => run(() => api("cash.close", { cemeteryId, date, countedHuf: Number(counted), note }))}>
                {t("finance.closeDay")}
              </Button>
            </div>
          </Panel>
        ))}
    </div>
  );
}

// ---------------------------------------------------------------- bank

function BankTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const q = useQuery({ queryKey: ["bank", cemeteryId], queryFn: () => api<BankTx[]>("bank.list", { cemeteryId, status: "unmatched" }) });
  const open = useQuery({ queryKey: ["invoices", cemeteryId, "open-for-bank"], queryFn: () => api<Invoice[]>("invoices.list", { cemeteryId }) });
  const openInvoices = (open.data ?? []).filter((i) => i.kind === "invoice" && ["issued", "partially_paid"].includes(i.status));
  const [pick, setPick] = useState<Record<string, string>>({});
  const [result, setResult] = useState<{ added: number; skipped: number } | null>(null);
  const [error, setError] = useState<unknown>(null);
  const run = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await invalidate();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <div className="flex flex-col gap-6">
      <Panel title={t("finance.bankImport")}>
        <div className="flex flex-col gap-3">
          <p className="text-granite">{t("finance.bankIntro")}</p>
          <label className="inline-flex w-fit cursor-pointer items-center gap-2 rounded-lg border border-seam bg-slab px-4 py-2 font-semibold">
            <Upload className="size-5" aria-hidden />
            {t("finance.bankPick")}
            <input
              type="file"
              accept=".csv,.txt,.xlsx"
              className="sr-only"
              data-testid="bank-file"
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (!f) return;
                const r = new FileReader();
                r.onload = () =>
                  void run(async () => setResult(await api<{ added: number; skipped: number }>("bank.import", { cemeteryId, fileName: f.name, content: String(r.result).split("base64,")[1] ?? "" })));
                r.readAsDataURL(f);
              }}
            />
          </label>
          {result && <Alert tone="ok">{t("finance.bankResult", result)}</Alert>}
          <ErrorAlert error={error} />
        </div>
      </Panel>
      <Panel title={t("finance.bankUnmatched")}>
        {(q.data ?? []).length === 0 ? (
          <Empty>{t("finance.bankNone")}</Empty>
        ) : (
          <table className="w-full min-w-[860px]" data-testid="bank-lines">
            <tbody>
              {(q.data ?? []).map((b) => {
                const chosen = pick[b.id] ?? b.suggestedInvoiceId ?? "";
                return (
                  <tr key={b.id} className="border-b border-seam last:border-0">
                    <td className={cx(td, "tabular-nums")}>{fmtDate(b.bookedOn)}</td>
                    <td className={td}>
                      <b>{b.partner}</b>
                      <span className="block text-sm text-granite">{b.reference}</span>
                    </td>
                    <td className={cx(td, "text-right font-semibold tabular-nums")}>{fmtHuf(b.amountHuf)}</td>
                    <td className={td}>
                      <Select aria-label={t("finance.matchTo")} value={chosen} onChange={(e) => setPick({ ...pick, [b.id]: e.target.value })}>
                        <option value="">{t("finance.chooseInvoice")}</option>
                        {openInvoices.map((i) => (
                          <option key={i.id} value={i.id}>
                            {`${i.number || t("finance.noNumber")} – ${i.buyerName} – ${fmtHuf(i.grossHuf - i.paidHuf)}`}
                          </option>
                        ))}
                      </Select>
                      {b.suggestedInvoiceNumber && <span className="block text-sm text-moss">{t("finance.suggested", { number: b.suggestedInvoiceNumber })}</span>}
                    </td>
                    <td className={cx(td, "pr-0 text-right")}>
                      <div className="flex justify-end gap-2">
                        <Button variant="primary" disabled={!chosen} onClick={() => run(() => api("bank.match", { id: b.id, invoiceId: chosen }))}>
                          {t("finance.match")}
                        </Button>
                        <Button variant="quiet" onClick={() => run(() => api("bank.match", { id: b.id, invoiceId: null }))}>
                          {t("finance.ignore")}
                        </Button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </Panel>
    </div>
  );
}

// ---------------------------------------------------------------- receivables

function ReceivablesTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["receivables", cemeteryId], queryFn: () => api<{ buyerName: string; invoices: number; openHuf: number; overdueHuf: number; oldestDue: string }[]>("receivables.list", { cemeteryId }) });
  const rows = q.data ?? [];
  const total = rows.reduce((s, r) => s + r.openHuf, 0);
  const overdue = rows.reduce((s, r) => s + r.overdueHuf, 0);
  return (
    <Panel title={t("finance.receivablesTitle", { total: fmtHuf(total), overdue: fmtHuf(overdue) })}>
      {rows.length === 0 ? (
        <Empty>{t("finance.noReceivables")}</Empty>
      ) : (
        <table className="w-full min-w-[640px]" data-testid="receivables">
          <thead>
            <tr className="border-b border-seam">
              <th className={th}>{t("finance.buyer")}</th>
              <th className={cx(th, "text-right")}>{t("finance.invoiceCount")}</th>
              <th className={cx(th, "text-right")}>{t("finance.open")}</th>
              <th className={cx(th, "text-right")}>{t("finance.overdueAmount")}</th>
              <th className={th}>{t("finance.oldestDue")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.buyerName} className="border-b border-seam last:border-0">
                <td className={cx(td, "font-semibold")}>{r.buyerName}</td>
                <td className={cx(td, "text-right tabular-nums")}>{r.invoices}</td>
                <td className={cx(td, "text-right tabular-nums")}>{fmtHuf(r.openHuf)}</td>
                <td className={cx(td, "text-right font-semibold tabular-nums", r.overdueHuf > 0 && "text-alert")}>{fmtHuf(r.overdueHuf)}</td>
                <td className={cx(td, "tabular-nums")}>{fmtDate(r.oldestDue)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Panel>
  );
}

// ---------------------------------------------------------------- report

function ReportTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const y = new Date().getFullYear();
  const [from, setFrom] = useState(`${y}-01-01`);
  const [to, setTo] = useState(`${y}-12-31`);
  const valid = /^\d{4}-\d{2}-\d{2}$/.test(from) && /^\d{4}-\d{2}-\d{2}$/.test(to);
  const q = useQuery({
    queryKey: ["revenue", cemeteryId, from, to],
    queryFn: () =>
      api<{
        lines: { ledgerCode: string; description: string; vatRate: string; netHuf: number; vatHuf: number; grossHuf: number }[];
        vatSummary: { vatRate: string; netHuf: number; vatHuf: number }[];
        totals: { netHuf: number; vatHuf: number; grossHuf: number; paidHuf: number };
      }>("finance.revenue", { cemeteryId, from, to }),
    enabled: valid,
  });
  const [error, setError] = useState<unknown>(null);
  const exp = async (kind: string) => {
    setError(null);
    try {
      await openGenerated(await api<GeneratedFile>("export.file", { kind, format: "xlsx", args: { cemeteryId, from, to } }));
    } catch (e) {
      setError(e);
    }
  };
  const d = q.data;
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end gap-4">
        <Field label={t("finance.from")}>
          <DateInput value={from} onValue={setFrom} />
        </Field>
        <Field label={t("finance.to")}>
          <DateInput value={to} onValue={setTo} />
        </Field>
        <Button onClick={() => exp("ledger")}>{t("finance.exportLedger")}</Button>
        <Button onClick={() => exp("asp")}>{t("finance.exportAsp")}</Button>
      </div>
      <ErrorAlert error={error} />
      {d && (
        <>
          <dl className="grid gap-4 sm:grid-cols-4" data-testid="revenue-totals">
            {(["netHuf", "vatHuf", "grossHuf", "paidHuf"] as const).map((k) => (
              <div key={k} className="rounded-xl border border-seam bg-slab px-4 py-3">
                <dt className="text-sm text-granite">{t(`finance.totals.${k}`)}</dt>
                <dd className="m-0 text-xl font-semibold tabular-nums">{fmtHuf(d.totals[k])}</dd>
              </div>
            ))}
          </dl>
          <Panel title={t("finance.byItem")}>
            {d.lines.length === 0 ? (
              <Empty>{t("finance.noRevenue")}</Empty>
            ) : (
              <table className="w-full min-w-[640px]">
                <thead>
                  <tr className="border-b border-seam">
                    <th className={th}>{t("finance.ledger")}</th>
                    <th className={th}>{t("finance.item")}</th>
                    <th className={th}>{t("finance.vat")}</th>
                    <th className={cx(th, "text-right")}>{t("finance.net")}</th>
                    <th className={cx(th, "text-right")}>{t("finance.gross")}</th>
                  </tr>
                </thead>
                <tbody>
                  {d.lines.map((l, i) => (
                    <tr key={i} className="border-b border-seam last:border-0">
                      <td className={cx(td, "tabular-nums")}>{l.ledgerCode}</td>
                      <td className={td}>{l.description}</td>
                      <td className={td}>{vatLabel(l.vatRate)}</td>
                      <td className={cx(td, "text-right tabular-nums")}>{fmtHuf(l.netHuf)}</td>
                      <td className={cx(td, "text-right tabular-nums")}>{fmtHuf(l.grossHuf)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Panel>
          <Panel title={t("finance.vatSummary")}>
            <table className="w-full">
              <tbody>
                {d.vatSummary.map((v) => (
                  <tr key={v.vatRate} className="border-b border-seam last:border-0">
                    <td className={td}>{vatLabel(v.vatRate)}</td>
                    <td className={cx(td, "text-right tabular-nums")}>{t("finance.netAmount", { amount: fmtHuf(v.netHuf) })}</td>
                    <td className={cx(td, "pr-0 text-right tabular-nums")}>{t("finance.vatAmount", { amount: fmtHuf(v.vatHuf) })}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Panel>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- setup

function SetupTab() {
  const { t } = useTranslation();
  const { status, refresh } = useApp();
  const invalidate = useInvalidate();
  const fs = useQuery({ queryKey: ["finance-status"], queryFn: () => api<FinanceStatus>("finance.status") });
  const [key, setKey] = useState("");
  const [due, setDue] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [saved, setSaved] = useState(false);
  const provider = fs.data?.provider ?? "szamlazz";
  const isAdmin = status?.session?.role === "admin";
  const save = async (fn: () => Promise<unknown>) => {
    setError(null);
    setSaved(false);
    try {
      await fn();
      await invalidate();
      await refresh();
      setSaved(true);
    } catch (e) {
      setError(e);
    }
  };
  const dueValue = useMemo(() => due ?? String(fs.data?.dueDays ?? 8), [due, fs.data]);
  return (
    <Panel title={t("finance.setupTitle")}>
      <div className="flex max-w-2xl flex-col gap-5">
        <Field label={t("finance.provider")} hint={provider === "asp" ? t("finance.providerAspHint") : t("finance.providerSzamlazzHint")}>
          <Select value={provider} disabled={!isAdmin} onChange={(e) => save(() => api("settings.set", { values: { "finance.provider": e.target.value } }))}>
            <option value="szamlazz">{t("finance.providers.szamlazz")}</option>
            <option value="asp">{t("finance.providers.asp")}</option>
          </Select>
        </Field>
        {provider === "szamlazz" && isAdmin && (
          <div className="flex flex-wrap items-end gap-3">
            <TextField className="min-w-72 flex-1" label={t("finance.agentKey")} type="password" value={key} onChange={setKey} hint={fs.data?.agentKeySet ? t("finance.agentKeySet") : t("finance.agentKeyMissing")} inputProps={{ autoComplete: "off" }} />
            <Button disabled={!key} onClick={() => save(async () => (await api("finance.setAgentKey", { key }), setKey("")))}>
              {t("common.save")}
            </Button>
          </div>
        )}
        <div className="flex flex-wrap items-end gap-3">
          <TextField className="w-48" label={t("finance.dueDays")} type="number" value={dueValue} onChange={setDue} />
          <Button onClick={() => save(() => api("settings.set", { values: { "finance.payment_due_days": dueValue } }))}>{t("common.save")}</Button>
        </div>
        <p className="text-sm text-granite">{t("finance.feeHint")}</p>
        {saved && <Alert tone="ok">{t("common.savedToast")}</Alert>}
        <ErrorAlert error={error} />
      </div>
    </Panel>
  );
}
