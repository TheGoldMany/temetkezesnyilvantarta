import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { command } from "../../lib/api";
import { Button, ErrorAlert, PageHeader, Panel, Spinner } from "../../components/ui";
import { huf } from "./Subscription";

interface AdminData {
  tenants: { id: string; name: string; status: string; trialEndsAt: string; createdAt: string; email: string; plan: string | null; paymentMethod: string | null; cemeteryCount: number | null; subscriptionStatus: string | null; periodEnd: string | null; users: number }[];
  openProformas: { id: string; tenant: string; number: string; grossHuf: number; dueDate: string | null; description: string; reference: string }[];
}

/** TEMISZ operators: every customer, open proformas to match with bank transfers. */
export function PlatformAdmin() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["platform"], queryFn: () => command<AdminData>("admin_overview") });
  const act = async (cmd: string, args: Record<string, unknown>) => {
    qc.setQueryData(["platform"], await command<AdminData>(cmd, args));
  };
  if (q.error) return <ErrorAlert error={q.error} />;
  if (!q.data) return <Spinner />;
  return (
    <>
      <PageHeader title={t("platform.title")} subtitle={t("platform.subtitle", { count: q.data.tenants.length })} />
      <div className="flex flex-col gap-6">
        <Panel title={t("platform.openProformas")}>
          {q.data.openProformas.length === 0 ? (
            <p className="text-granite">{t("platform.noOpen")}</p>
          ) : (
            <table className="w-full" data-testid="open-proformas">
              <tbody>
                {q.data.openProformas.map((p) => (
                  <tr key={p.id} className="border-b border-seam last:border-0">
                    <td className="py-2.5 pr-4 font-semibold">{p.number}</td>
                    <td className="py-2.5 pr-4">
                      {p.tenant}
                      <span className="block text-sm text-granite">
                        {t("platform.reference")}: {p.reference}, {p.description}
                      </span>
                    </td>
                    <td className="py-2.5 pr-4 text-right tabular-nums">{huf(p.grossHuf)}</td>
                    <td className="py-2.5 pr-4 tabular-nums">{p.dueDate}</td>
                    <td className="py-2.5 text-right">
                      <Button variant="primary" onClick={() => act("admin_mark_paid", { invoiceId: p.id })}>
                        {t("platform.markPaid")}
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Panel>
        <Panel title={t("platform.customers")}>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[900px]">
              <thead>
                <tr className="border-b border-seam text-left text-sm text-granite">
                  <th className="py-2 pr-4 font-medium">{t("platform.customer")}</th>
                  <th className="py-2 pr-4 font-medium">{t("billing.state")}</th>
                  <th className="py-2 pr-4 font-medium">{t("platform.subscription")}</th>
                  <th className="py-2 pr-4 font-medium">{t("platform.users")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {q.data.tenants.map((x) => (
                  <tr key={x.id} className="border-b border-seam last:border-0">
                    <td className="py-2.5 pr-4">
                      <span className="font-semibold">{x.name}</span>
                      <span className="block text-sm text-granite">
                        {x.email}, {x.createdAt}
                      </span>
                    </td>
                    <td className="py-2.5 pr-4">{x.status === "trial" ? t("platform.trialUntil", { date: x.trialEndsAt }) : t(`platform.status.${x.status}`)}</td>
                    <td className="py-2.5 pr-4">{x.plan ? `${t(`billing.plan.${x.plan}`)}, ${t(`billing.method.${x.paymentMethod}`)}, ${t("billing.cemeteries", { count: x.cemeteryCount ?? 0 })}, ${x.subscriptionStatus}${x.periodEnd ? `, ${x.periodEnd}` : ""}` : "–"}</td>
                    <td className="py-2.5 pr-4 tabular-nums">{x.users}</td>
                    <td className="py-2.5 text-right">
                      <Button onClick={() => act("admin_extend_trial", { tenantId: x.id, days: 14 })}>{t("platform.extendTrial")}</Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      </div>
    </>
  );
}
