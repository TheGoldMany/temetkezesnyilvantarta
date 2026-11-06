import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FileText, Landmark } from "lucide-react";
import { command } from "../../lib/api";
import { Alert, Button, ErrorAlert, Panel, Spinner, TextField, cx } from "../../components/ui";
import { openGenerated, type GeneratedFile } from "../prints/PrintButton";
import { useApp } from "../../app/context";

interface Price {
  unitNet: number;
  net: number;
  gross: number;
}
interface Overview {
  status: "trial" | "active" | "read_only" | "closed";
  trialEndsAt: string;
  trialDaysLeft: number;
  cemeteryCount: number;
  vatPercent: number;
  prices: { monthly: Price; yearly: Price };
  details: { name: string; taxNumber: string; postcode: string; city: string; address: string; email: string } | null;
  subscription: {
    plan: "monthly" | "yearly";
    paymentMethod: "transfer";
    cemeteryCount: number;
    status: "pending" | "active" | "past_due" | "canceled";
    periodStart: string | null;
    periodEnd: string | null;
    cancelAtPeriodEnd: boolean;
  } | null;
  invoices: { id: string; kind: "proforma" | "invoice"; number: string; description: string; grossHuf: number; date: string; dueDate: string | null; status: string; hasPdf: boolean }[];
}

export const huf = (n: number) => `${n.toLocaleString("hu-HU")} Ft`;

/** Settings › Előfizetés: price, billing details, payment method, invoices. */
export function SubscriptionTab() {
  const { t } = useTranslation();
  const { refresh } = useApp();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["billing"], queryFn: () => command<Overview>("billing_overview") });
  const [plan, setPlan] = useState<"monthly" | "yearly">("yearly");
  const [editDetails, setEditDetails] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [done, setDone] = useState(false);
  if (q.isLoading || !q.data) return q.error ? <ErrorAlert error={q.error} /> : <Spinner />;
  const o = q.data;
  const sub = o.subscription && o.subscription.status !== "canceled" ? o.subscription : null;
  const reload = async () => {
    await qc.invalidateQueries({ queryKey: ["billing"] });
    await refresh();
  };
  const subscribe = async () => {
    setBusy(true);
    setError(null);
    try {
      await command("billing_subscribe", { plan, paymentMethod: "transfer" });
      setDone(true);
      await reload();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-6">
      <Panel title={t("billing.statusTitle")}>
        <div className="flex flex-col gap-2" data-testid="billing-status">
          {o.status === "trial" && !sub && <p>{t("billing.inTrial", { count: o.trialDaysLeft, date: o.trialEndsAt })}</p>}
          {o.status === "read_only" && <Alert tone="warn">{t("billing.readOnly")}</Alert>}
          {sub && (
            <p>
              {t(`billing.plan.${sub.plan}`)}, {t(`billing.method.${sub.paymentMethod}`)}, {t("billing.cemeteries", { count: sub.cemeteryCount })}.{" "}
              {sub.status === "pending" ? t("billing.pending") : sub.periodEnd ? t("billing.paidUntil", { date: sub.periodEnd }) : ""}
              {sub.cancelAtPeriodEnd && ` ${t("billing.cancelAtEnd")}`}
            </p>
          )}
          {done && <Alert tone="ok">{t("billing.proformaSent")}</Alert>}
        </div>
      </Panel>

      {!sub && (
        <Panel title={t("billing.chooseTitle")}>
          <div className="flex flex-col gap-5">
            <p className="text-granite">{t("billing.priceIntro", { count: o.cemeteryCount })}</p>
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
                  {p === "yearly" && <span className="mt-2 inline-block rounded-full bg-lichen px-2 py-0.5 text-sm font-semibold text-accent-ink">{t("billing.yearlySaving")}</span>}
                </button>
              ))}
            </div>

            <div>
              <p className="mb-1 flex items-center gap-2 font-semibold">
                <Landmark className="size-5" aria-hidden />
                {t("billing.paymentMethod")}: {t("billing.method.transfer")}
              </p>
              <p className="text-sm text-granite">{t("billing.transferHint")}</p>
            </div>

            {(!o.details || editDetails) && <DetailsForm details={o.details} onSaved={async () => (setEditDetails(false), await reload())} />}
            {o.details && !editDetails && (
              <div className="rounded-lg border border-seam bg-limestone/60 px-4 py-3">
                <p className="text-sm text-granite">{t("billing.invoiceTo")}</p>
                <p className="font-semibold">{o.details.name}</p>
                <p>
                  {o.details.postcode} {o.details.city}, {o.details.address}
                  {o.details.taxNumber && `, ${t("billing.taxNumber")}: ${o.details.taxNumber}`}
                </p>
                <p className="text-granite">{o.details.email}</p>
                <button type="button" className="mt-1 text-moss underline underline-offset-3" onClick={() => setEditDetails(true)}>
                  {t("common.edit")}
                </button>
              </div>
            )}
            <ErrorAlert error={error} />
            <div>
              <Button variant="primary" size="lg" busy={busy} disabled={!o.details || editDetails} onClick={subscribe}>
                {t("billing.subscribeTransfer")}
              </Button>
            </div>
          </div>
        </Panel>
      )}

      {sub && o.details && (
        <Panel title={t("billing.detailsTitle")}>
          {editDetails ? (
            <DetailsForm details={o.details} onSaved={async () => (setEditDetails(false), await reload())} />
          ) : (
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p>
                {o.details.name}, {o.details.postcode} {o.details.city}, {o.details.address} ({o.details.email})
              </p>
              <Button onClick={() => setEditDetails(true)}>{t("common.edit")}</Button>
            </div>
          )}
        </Panel>
      )}

      <Panel title={t("billing.invoicesTitle")}>
        {o.invoices.length === 0 ? (
          <p className="text-granite">{t("billing.noInvoices")}</p>
        ) : (
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
                  <td className="py-2.5 text-right">
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
        )}
      </Panel>
      <CloseAccount />
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
      <TextField className="md:col-span-1" label={t("billing.postcode")} value={v.postcode} onChange={set("postcode")} required inputProps={{ name: "postcode" }} />
      <TextField className="md:col-span-2" label={t("billing.city")} value={v.city} onChange={set("city")} required inputProps={{ name: "city" }} />
      <TextField className="md:col-span-3" label={t("billing.address")} value={v.address} onChange={set("address")} required inputProps={{ name: "address" }} />
      <TextField className="md:col-span-3" label={t("billing.email")} type="email" value={v.email} onChange={set("email")} required hint={t("billing.emailHint")} inputProps={{ name: "billingEmail" }} />
      <div className="md:col-span-6">
        <ErrorAlert error={error} />
        <Button type="submit" className="mt-2">
          {t("billing.saveDetails")}
        </Button>
      </div>
    </form>
  );
}
