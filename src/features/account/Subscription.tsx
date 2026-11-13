import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CreditCard, FileText, Landmark } from "lucide-react";
import { command } from "../../lib/api";
import { Alert, Button, ErrorAlert, Panel, Spinner, TextField, cx } from "../../components/ui";
import { openGenerated, type GeneratedFile } from "../prints/PrintButton";
import { useApp } from "../../app/context";
import { BarionBanner } from "../../components/BarionBanner";

interface Price {
  unitNet: number;
  net: number;
  gross: number;
}
interface CemeterySub {
  cemeteryId: string;
  name: string;
  status: "none" | "pending_card" | "trial" | "active" | "past_due" | "canceled" | "lapsed" | "demo";
  plan?: "monthly" | "yearly";
  method?: "card" | "transfer" | "demo";
  trialEndsAt?: string | null;
  periodEnd?: string | null;
  cancelAtPeriodEnd?: boolean;
  /** the first payment arrived (exports allowed) */
  paid: boolean;
  /** a proforma of the cemetery waits for payment */
  openProforma?: boolean;
}
interface Overview {
  cardPayments: boolean;
  vatPercent: number;
  prices: { monthly: Price; yearly: Price };
  details: { name: string; taxNumber: string; postcode: string; city: string; address: string; email: string } | null;
  cemeteries: CemeterySub[];
  trialDays: number;
  demoDays: number;
  verifyHuf: number;
  /** card subscriptions (token payments) are enabled at Barion */
  cardSubscriptions: boolean;
  /** an earlier free period ended unpaid: no second one */
  trialUsed: boolean;
  invoices: { id: string; kind: "proforma" | "invoice"; number: string; description: string; grossHuf: number; date: string; dueDate: string | null; status: string; hasPdf: boolean }[];
}

export const huf = (n: number) => `${n.toLocaleString("hu-HU")} Ft`;

/** Pays an open proforma by card on Barion's payment page (the browser goes there and comes back). */
function PayByCard({ invoiceId, accepted }: { invoiceId: string; accepted: boolean }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  return (
    <span className="mr-2 inline-flex flex-col items-end gap-1 align-middle">
      <Button
        variant="primary"
        icon={<CreditCard className="size-4" aria-hidden />}
        disabled={busy || !accepted}
        title={accepted ? undefined : t("billing.acceptFirst")}
        onClick={async () => {
          setBusy(true);
          setError(null);
          try {
            const r = await command<{ gatewayUrl: string }>("billing_pay_card", { invoiceId, termsAccepted: accepted });
            window.location.assign(r.gatewayUrl);
          } catch (e) {
            setError(e);
            setBusy(false);
          }
        }}
      >
        {busy ? t("billing.cardOpening") : t("billing.payByCard")}
      </Button>
      <ErrorAlert error={error} />
    </span>
  );
}

/** The buyer accepts the ÁSZF before ordering or paying (card schemes and Barion require it). */
function TermsCheck({ checked, onChange, card }: { checked: boolean; onChange: (v: boolean) => void; card?: boolean }) {
  const { t } = useTranslation();
  return (
    <label className="flex max-w-3xl items-start gap-3 text-[0.9375rem]">
      <input type="checkbox" className="mt-1 size-4 shrink-0 accent-[var(--c-moss)]" checked={checked} onChange={(e) => onChange(e.target.checked)} data-testid={card ? "terms-card" : "terms"} />
      <span>
        {t("billing.acceptBefore")}{" "}
        <a className="text-moss underline underline-offset-3" href="/aszf.html" target="_blank" rel="noreferrer">
          {t("billing.acceptTerms")}
        </a>
        {t("billing.acceptAnd")}{" "}
        <a className="text-moss underline underline-offset-3" href="/adatkezeles.html" target="_blank" rel="noreferrer">
          {t("billing.acceptPrivacy")}
        </a>
        {card ? t("billing.acceptCard") : t("billing.acceptAfter")}
      </span>
    </label>
  );
}

/** Settings › Előfizetés: one subscription per cemetery, billing details, invoices. */
export function SubscriptionTab() {
  const { t } = useTranslation();
  const { refresh } = useApp();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["billing"], queryFn: () => command<Overview>("billing_overview") });
  const [editDetails, setEditDetails] = useState(false);
  const [cardAccepted, setCardAccepted] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  if (q.isLoading || !q.data) return q.error ? <ErrorAlert error={q.error} /> : <Spinner />;
  const o = q.data;
  const reload = async () => {
    await qc.invalidateQueries({ queryKey: ["billing"] });
    await refresh();
  };
  return (
    <div className="flex flex-col gap-6">
      <Panel title={t("billing.cemeteriesTitle")}>
        <div className="flex flex-col gap-4" data-testid="billing-status">
          <p className="text-granite">{t("billing.perCemeteryIntro", { days: o.trialDays, monthly: huf(o.prices.monthly.net), yearly: huf(o.prices.yearly.net) })}</p>
          {o.cemeteries.length === 0 ? (
            <p className="text-granite">{t("billing.noCemeteries")}</p>
          ) : (
            <ul className="flex flex-col divide-y divide-seam border-y border-seam" data-testid="cemetery-subs">
              {o.cemeteries.map((c) => (
                <li key={c.cemeteryId} className="flex flex-col gap-3 py-3">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <p className="font-semibold">{c.name}</p>
                      <p className={cx("text-sm", ["none", "lapsed", "past_due"].includes(c.status) ? "font-semibold text-alert" : "text-granite")}>{subText(t, c)}</p>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {["none", "lapsed", "demo"].includes(c.status) && !(c.status === "lapsed" && c.openProforma) && open !== c.cemeteryId && (
                        <Button variant="primary" onClick={() => setOpen(c.cemeteryId)}>
                          {t("billing.subscribeCemetery")}
                        </Button>
                      )}
                      {["trial", "active", "past_due"].includes(c.status) && <CancelButton cemeteryId={c.cemeteryId} onDone={reload} />}
                      {c.status === "canceled" && <CancelButton cemeteryId={c.cemeteryId} resume onDone={reload} />}
                    </div>
                  </div>
                  {open === c.cemeteryId && (
                    <SubscribeForm o={o} cemetery={c} onClose={() => setOpen(null)} onDone={async () => (setOpen(null), await reload())} />
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      </Panel>

      <Panel title={t("billing.detailsTitle")}>
        {!o.details || editDetails ? (
          <DetailsForm details={o.details} onSaved={async () => (setEditDetails(false), await reload())} />
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p>
              {o.details.name}, {o.details.postcode} {o.details.city}, {o.details.address}
              {o.details.taxNumber && `, ${t("billing.taxNumber")}: ${o.details.taxNumber}`} ({o.details.email})
            </p>
            <Button onClick={() => setEditDetails(true)}>{t("common.edit")}</Button>
          </div>
        )}
      </Panel>

      <Panel title={t("billing.invoicesTitle")}>
        {o.invoices.length === 0 ? (
          <p className="text-granite">{t("billing.noInvoices")}</p>
        ) : (
          <>
          {o.cardPayments && o.invoices.some((i) => i.kind === "proforma" && i.status === "open") && (
            <div className="mb-4">
              <TermsCheck checked={cardAccepted} onChange={setCardAccepted} card />
              <p className="mt-3 text-sm text-granite">{t("billing.cardVia")}</p>
              <BarionBanner className="mt-2 block h-10 w-auto max-w-full" />
            </div>
          )}
          <table className="w-full" data-testid="invoices">
            <thead>
              <tr className="border-b border-seam text-left text-sm text-granite">
                <th className="py-2 pr-4 font-medium">{t("billing.number")}</th>
                <th className="py-2 pr-4 font-medium">{t("common.date")}</th>
                <th className="py-2 pr-4 font-medium">{t("billing.item")}</th>
                <th className="py-2 pr-4 text-right font-medium">{t("billing.amount")}</th>
                <th className="py-2 font-medium">{t("billing.state")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {o.invoices.map((i) => (
                <tr key={i.id} className="border-b border-seam last:border-0">
                  <td className="py-2.5 pr-4 font-semibold tabular-nums">
                    {i.number}
                    <span className="block text-sm font-normal text-granite">{t(`billing.kind.${i.kind}`)}</span>
                  </td>
                  <td className="py-2.5 pr-4 tabular-nums">{i.date}</td>
                  <td className="py-2.5 pr-4">{i.description}</td>
                  <td className="py-2.5 pr-4 text-right tabular-nums">{huf(i.grossHuf)}</td>
                  <td className="py-2.5">{i.status === "open" ? t("billing.openUntil", { date: i.dueDate }) : t(`billing.status.${i.status}`)}</td>
                  <td className="py-2.5 text-right whitespace-nowrap">
                    {o.cardPayments && i.kind === "proforma" && i.status === "open" && <PayByCard invoiceId={i.id} accepted={cardAccepted} />}
                    {i.hasPdf && (
                      <Button variant="quiet" icon={<FileText className="size-4" aria-hidden />} onClick={async () => openGenerated(await command<GeneratedFile>("billing_invoice_pdf", { id: i.id }))}>
                        PDF
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          </>
        )}
      </Panel>
      <CloseAccount />
    </div>
  );
}

function subText(t: (k: string, o?: Record<string, unknown>) => string, c: CemeterySub): string {
  const per = c.plan ? t(`billing.per.${c.plan}`) : "";
  switch (c.status) {
    case "none":
      return t("billing.sub.none");
    case "trial":
      return c.method === "card" ? t("billing.sub.trialCard", { date: c.trialEndsAt, per }) : t("billing.sub.trialOld", { date: c.trialEndsAt });
    case "active":
      if (c.method === "card") return t("billing.sub.activeCard", { date: c.periodEnd, per });
      return c.paid ? t("billing.sub.activeTransfer", { date: c.periodEnd }) : t("billing.sub.awaitingTransfer");
    case "past_due":
      return t("billing.sub.pastDue");
    case "canceled":
      return t("billing.sub.canceled", { date: c.periodEnd ?? c.trialEndsAt });
    case "demo":
      return t("billing.sub.demo", { date: c.trialEndsAt });
    default:
      return c.openProforma ? t("billing.sub.prepay") : t("billing.sub.lapsed");
  }
}

function CancelButton({ cemeteryId, resume, onDone }: { cemeteryId: string; resume?: boolean; onDone: () => Promise<void> }) {
  const { t } = useTranslation();
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const run = async () => {
    setError(null);
    try {
      await command("cemetery_cancel", { cemeteryId, resume: !!resume });
      setConfirm(false);
      await onDone();
    } catch (e) {
      setError(e);
    }
  };
  if (resume) return <Button onClick={run}>{t("billing.resume")}</Button>;
  return (
    <span className="flex flex-col items-end gap-2">
      {confirm ? (
        <span className="flex flex-wrap items-center gap-2">
          <span className="text-sm">{t("billing.cancelConfirm")}</span>
          <Button onClick={() => setConfirm(false)}>{t("common.cancel")}</Button>
          <Button variant="danger" onClick={run}>
            {t("billing.cancelFinal")}
          </Button>
        </span>
      ) : (
        <Button variant="quiet" onClick={() => setConfirm(true)}>
          {t("billing.cancelSub")}
        </Button>
      )}
      <ErrorAlert error={error} />
    </span>
  );
}

/** Plan and payment method for one cemetery: card (check, trial, automatic charges) or transfer (proforma now). */
function SubscribeForm({ o, cemetery, onClose, onDone }: { o: Overview; cemetery: CemeterySub; onClose: () => void; onDone: () => Promise<void> }) {
  const { t } = useTranslation();
  const [plan, setPlan] = useState<"monthly" | "yearly">("yearly");
  const [method, setMethod] = useState<"card" | "transfer">(o.cardSubscriptions ? "card" : "transfer");
  const [accepted, setAccepted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  // a free period only once: not after this cemetery's trial, nor after an unpaid one elsewhere
  const trialUsed = o.trialUsed || (cemetery.status === "lapsed" && !!cemetery.trialEndsAt && cemetery.method !== "demo");
  return (
    <div className="flex flex-col gap-4 rounded-lg border border-seam bg-limestone/50 p-4" data-testid="subscribe-form">
      <div className="grid gap-3 md:grid-cols-2" role="radiogroup" aria-label={t("billing.chooseTitle")}>
        {(["monthly", "yearly"] as const).map((p) => (
          <button
            key={p}
            type="button"
            role="radio"
            aria-checked={plan === p}
            onClick={() => setPlan(p)}
            className={cx("rounded-xl border p-4 text-left", plan === p ? "border-moss bg-moss/8 shadow-[inset_0_0_0_1px_var(--c-moss)]" : "border-seam bg-slab hover:border-granite")}
          >
            <span className="block font-semibold">{t(`billing.plan.${p}`)}</span>
            <span className="mt-1 block text-2xl font-semibold tabular-nums">{huf(o.prices[p].net)}</span>
            <span className="block text-sm text-granite">
              {t("billing.netPer", { per: t(`billing.per.${p}`) })}, {t("billing.gross", { amount: huf(o.prices[p].gross) })}
            </span>
            {p === "yearly" && <span className="mt-2 inline-block bg-lichen px-2 py-0.5 text-sm font-semibold text-accent-ink">{t("billing.yearlySaving")}</span>}
          </button>
        ))}
      </div>
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-1 font-semibold">{t("billing.paymentMethod")}</legend>
        {o.cardSubscriptions && (
          <label className="flex items-start gap-3">
            <input type="radio" name="method" className="mt-1 size-4 accent-[var(--c-moss)]" checked={method === "card"} onChange={() => setMethod("card")} />
            <span>
              <span className="flex items-center gap-2 font-semibold">
                <CreditCard className="size-4" aria-hidden />
                {t("billing.method.card")}
              </span>
              <span className="block text-sm text-granite">{trialUsed ? t("billing.cardHintNoTrial", { verify: huf(o.verifyHuf) }) : t("billing.cardHint", { days: o.trialDays, verify: huf(o.verifyHuf) })}</span>
              {method === "card" && <BarionBanner className="mt-2 block h-9 w-auto max-w-full" />}
            </span>
          </label>
        )}
        <label className="flex items-start gap-3">
          <input type="radio" name="method" className="mt-1 size-4 accent-[var(--c-moss)]" checked={method === "transfer"} onChange={() => setMethod("transfer")} />
          <span>
            <span className="flex items-center gap-2 font-semibold">
              <Landmark className="size-4" aria-hidden />
              {t("billing.method.transfer")}
            </span>
            <span className="block text-sm text-granite">{o.trialUsed ? t("billing.transferHintPrepay") : t("billing.transferHint")}</span>
          </span>
        </label>
      </fieldset>
      {!o.details && <Alert tone="warn">{t("billing.detailsFirst")}</Alert>}
      <TermsCheck checked={accepted} onChange={setAccepted} card={method === "card"} />
      <ErrorAlert error={error} />
      <div className="flex flex-wrap gap-3">
        <Button onClick={onClose}>{t("common.cancel")}</Button>
        <Button
          variant="primary"
          busy={busy}
          disabled={!o.details || !accepted}
          onClick={async () => {
            setBusy(true);
            setError(null);
            try {
              const r = await command<{ gatewayUrl?: string }>("cemetery_subscribe", { cemeteryId: cemetery.cemeteryId, plan, paymentMethod: method, termsAccepted: accepted });
              if (r.gatewayUrl) {
                window.location.assign(r.gatewayUrl);
                return;
              }
              await onDone();
            } catch (e) {
              setError(e);
            } finally {
              setBusy(false);
            }
          }}
        >
          {method === "card" ? t("billing.subscribeCard") : t("billing.subscribeTransfer")}
        </Button>
      </div>
    </div>
  );
}

/** Closing the account: sign-in stops now, data are deleted after 30 days. */
function CloseAccount() {
  const { t } = useTranslation();
  const { setStatus } = useApp();
  const [open, setOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<unknown>(null);
  return (
    <Panel title={t("billing.closeTitle")}>
      <div className="flex max-w-2xl flex-col gap-4">
        <p className="text-granite">{t("billing.closeIntro")}</p>
        {!open ? (
          <div>
            <Button variant="danger" onClick={() => setOpen(true)}>
              {t("billing.closeStart")}
            </Button>
          </div>
        ) : (
          <form
            className="flex flex-col gap-4"
            onSubmit={async (e) => {
              e.preventDefault();
              setError(null);
              try {
                setStatus(await command("close_account", { password }));
              } catch (err) {
                setError(err);
              }
            }}
          >
            <label className="flex items-start gap-3">
              <input type="checkbox" className="mt-1 size-5 accent-[var(--c-alert)]" checked={confirm} onChange={(e) => setConfirm(e.target.checked)} />
              <span>{t("billing.closeConfirm")}</span>
            </label>
            <TextField className="max-w-sm" label={t("billing.closePassword")} type="password" value={password} onChange={setPassword} inputProps={{ name: "closePassword" }} />
            <ErrorAlert error={error} />
            <div className="flex gap-3">
              <Button onClick={() => setOpen(false)}>{t("common.cancel")}</Button>
              <Button type="submit" variant="danger" disabled={!confirm || !password}>
                {t("billing.closeFinal")}
              </Button>
            </div>
          </form>
        )}
      </div>
    </Panel>
  );
}

function DetailsForm({ details, onSaved }: { details: Overview["details"]; onSaved: () => Promise<void> }) {
  const { t } = useTranslation();
  const { status } = useApp();
  const [v, setV] = useState(details ?? { name: status?.account?.organization ?? "", taxNumber: "", postcode: "", city: "", address: "", email: status?.account?.email ?? "" });
  const [error, setError] = useState<unknown>(null);
  const set = (k: keyof typeof v) => (x: string) => setV({ ...v, [k]: x });
  return (
    <form
      className="grid gap-4 md:grid-cols-6"
      onSubmit={async (e) => {
        e.preventDefault();
        setError(null);
        try {
          await command("billing_save_details", v);
          await onSaved();
        } catch (err) {
          setError(err);
        }
      }}
    >
      <p className="font-semibold md:col-span-6">{t("billing.detailsTitle")}</p>
      <TextField className="md:col-span-4" label={t("billing.name")} value={v.name} onChange={set("name")} required inputProps={{ name: "billingName" }} />
      <TextField className="md:col-span-2" label={t("billing.taxNumber")} value={v.taxNumber} onChange={set("taxNumber")} hint={t("billing.taxHint")} inputProps={{ name: "taxNumber" }} />
      <TextField className="md:col-span-2" label={t("billing.postcode")} value={v.postcode} onChange={set("postcode")} required inputProps={{ name: "postcode" }} />
      <TextField className="md:col-span-4" label={t("billing.city")} value={v.city} onChange={set("city")} required inputProps={{ name: "city" }} />
      <TextField className="md:col-span-6" label={t("billing.address")} value={v.address} onChange={set("address")} required inputProps={{ name: "address" }} />
      <TextField className="md:col-span-6" label={t("billing.email")} type="email" value={v.email} onChange={set("email")} required hint={t("billing.emailHint")} inputProps={{ name: "billingEmail" }} />
      <div className="md:col-span-6">
        <ErrorAlert error={error} />
        <Button type="submit" className="mt-2">
          {t("billing.saveDetails")}
        </Button>
      </div>
    </form>
  );
}
