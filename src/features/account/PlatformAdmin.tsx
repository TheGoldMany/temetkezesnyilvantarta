import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { command } from "../../lib/api";
import { Alert, Button, ErrorAlert, PageHeader, Panel, Spinner } from "../../components/ui";
import { huf } from "./Subscription";
import { openGenerated, type GeneratedFile } from "../prints/PrintButton";

interface AdminData {
  tenants: {
    id: string;
    name: string;
    status: string;
    createdAt: string;
    email: string;
    users: number;
    cemeteries: { name: string; status: string; plan: string; method: string; until: string | null; paid: boolean }[];
  }[];
  openProformas: { id: string; tenant: string; number: string; grossHuf: number; dueDate: string | null; description: string; reference: string }[];
  bankLines: { id: string; bookedOn: string; amountHuf: number; partner: string; reference: string; suggestedId: string | null; suggestedNumber: string | null; suggestedBuyer: string | null; suggestedGross: number | null }[];
  import?: { added: number; skipped: number; matched: number };
}

/** TEMISZ operators: every customer, open proformas to match with bank transfers. */
export function PlatformAdmin() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["platform"], queryFn: () => command<AdminData>("admin_overview") });
  const [error, setError] = useState<unknown>(null);
  const [imported, setImported] = useState<AdminData["import"] | null>(null);
  const act = async (cmd: string, args: Record<string, unknown>) => {
    setError(null);
    try {
      const d = await command<AdminData>(cmd, args);
      qc.setQueryData(["platform"], d);
      if (d.import) setImported(d.import);
    } catch (e) {
      setError(e);
    }
  };
  if (q.error) return <ErrorAlert error={q.error} />;
  if (!q.data) return <Spinner />;
  return (
    <>
      <PageHeader title={t("platform.title")} subtitle={t("platform.subtitle", { count: q.data.tenants.length })} />
      <div className="flex flex-col gap-6">
        <Panel
          title={t("platform.bank")}
          actions={
            <label className="inline-flex min-h-11 cursor-pointer items-center rounded-lg border border-moss bg-moss px-4 font-semibold text-moss-ink hover:bg-moss/90">
              {t("platform.importStatement")}
              <input
                type="file"
                accept=".csv,.xlsx,.xls,text/csv"
                className="sr-only"
                name="statement"
                onChange={async (e) => {
                  const f = e.target.files?.[0];
                  e.target.value = "";
                  if (!f) return;
                  const buf = new Uint8Array(await f.arrayBuffer());
                  let bin = "";
                  for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
                  await act("admin_bank_import", { fileName: f.name, content: btoa(bin) });
                }}
              />
            </label>
          }
        >
          <div className="flex flex-col gap-3">
            <p className="text-granite">{t("platform.bankHint")}</p>
            <ErrorAlert error={error} />
            {imported && <Alert tone="ok">{t("platform.imported", imported)}</Alert>}
            {q.data.bankLines.length > 0 && (
              <table className="w-full min-w-[720px]" data-testid="bank-lines">
                <tbody>
                  {q.data.bankLines.map((l) => (
                    <tr key={l.id} className="border-b border-seam last:border-0">
                      <td className="py-2.5 pr-4 tabular-nums">{l.bookedOn}</td>
                      <td className="py-2.5 pr-4">
                        <span className="font-semibold">{l.partner || "–"}</span>
                        <span className="block text-sm text-granite">{l.reference}</span>
                      </td>
                      <td className="py-2.5 pr-4 text-right tabular-nums">{huf(l.amountHuf)}</td>
                      <td className="py-2.5 pr-4 text-sm">{l.suggestedNumber ? t("platform.suggested", { number: l.suggestedNumber, buyer: l.suggestedBuyer, amount: huf(l.suggestedGross ?? 0) }) : t("platform.noSuggestion")}</td>
                      <td className="py-2.5 text-right">
                        <div className="flex justify-end gap-2">
                          {l.suggestedId && (
                            <Button variant="primary" onClick={() => act("admin_bank_resolve", { lineId: l.id, invoiceId: l.suggestedId })}>
                              {t("platform.confirmMatch")}
                            </Button>
                          )}
                          <Button variant="quiet" onClick={() => act("admin_bank_resolve", { lineId: l.id, invoiceId: null })}>
                            {t("platform.ignore")}
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </Panel>
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
        <DemoRequests />
        <Panel title={t("platform.customers")}>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[900px]">
              <thead>
                <tr className="border-b border-seam text-left text-sm text-granite">
                  <th className="py-2 pr-4 font-medium">{t("platform.customer")}</th>
                  <th className="py-2 pr-4 font-medium">{t("billing.state")}</th>
                  <th className="py-2 pr-4 font-medium">{t("platform.cemeterySubs")}</th>
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
                    <td className="py-2.5 pr-4">{t(`platform.status.${x.status}`, { defaultValue: x.status })}</td>
                    <td className="py-2.5 pr-4">
                      {x.cemeteries.length === 0 ? (
                        <span className="text-granite">{t("platform.noCemeterySubs")}</span>
                      ) : (
                        <ul className="flex flex-col gap-0.5">
                          {x.cemeteries.map((c) => (
                            <li key={c.name}>
                              <span className="font-medium">{c.name}</span>
                              <span className="text-granite">
                                {": "}
                                {t(`platform.sub.${c.status}`, { defaultValue: c.status })}
                                {c.status === "active" && !c.paid ? ` (${t("platform.unpaid")})` : ""}
                                {c.until ? `, ${c.until}` : ""}
                                {c.method !== "demo" ? `, ${t(`billing.plan.${c.plan}`)}, ${t(`billing.method.${c.method}`)}` : ""}
                              </span>
                            </li>
                          ))}
                        </ul>
                      )}
                    </td>
                    <td className="py-2.5 pr-4 tabular-nums">{x.users}</td>
                    <td className="py-2.5 text-right">
                      {x.cemeteries.some((c) => c.status === "trial" || (c.status === "lapsed" && !c.paid)) && <Button onClick={() => act("admin_extend_trial", { tenantId: x.id, days: 14 })}>{t("platform.extendTrial")}</Button>}
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

interface DemoRequest {
  id: string;
  createdAt: string;
  settlement: string;
  name: string;
  role: string;
  email: string;
  phone: string;
  message: string;
  fileName: string;
  fileSize: number;
  done: boolean;
}

/** temisz.hu/bemutato: uploaded tables for the free migration demo (deleted after 30 days). */
function DemoRequests() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["demo-requests"], queryFn: () => command<DemoRequest[]>("admin_demo_requests") });
  const [error, setError] = useState<unknown>(null);
  const act = async (cmd: string, id: string) => {
    setError(null);
    try {
      qc.setQueryData(["demo-requests"], await command<DemoRequest[]>(cmd, { id }));
    } catch (e) {
      setError(e);
    }
  };
  const rows = q.data ?? [];
  return (
    <Panel title={t("platform.demoTitle")}>
      <p className="mb-3 text-sm text-granite">{t("platform.demoHint")}</p>
      <ErrorAlert error={error ?? q.error} />
      {rows.length === 0 ? (
        <p className="text-granite">{t("platform.demoNone")}</p>
      ) : (
        <ul className="flex flex-col divide-y divide-seam border-y border-seam" data-testid="demo-requests">
          {rows.map((r) => (
            <li key={r.id} className="flex flex-wrap items-start justify-between gap-3 py-3">
              <div className="min-w-0">
                <p className="font-semibold">
                  {r.settlement} {r.done && <span className="ml-2 text-sm font-normal text-moss">{t("platform.demoDone")}</span>}
                </p>
                <p className="text-sm">
                  {r.name}
                  {r.role && ` (${r.role})`} · <a className="text-moss underline" href={`mailto:${r.email}`}>{r.email}</a>
                  {r.phone && ` · ${r.phone}`}
                </p>
                <p className="text-sm text-granite">
                  {new Date(r.createdAt).toLocaleString("hu-HU")} · {r.fileName} ({Math.ceil(r.fileSize / 1024)} kB)
                </p>
                {r.message && <p className="mt-1 text-sm whitespace-pre-line">{r.message}</p>}
              </div>
              <div className="flex flex-wrap gap-2">
                <Button onClick={async () => openGenerated(await command<GeneratedFile>("admin_demo_download", { id: r.id }))}>{t("platform.demoDownload")}</Button>
                {!r.done && <Button onClick={() => act("admin_demo_done", r.id)}>{t("platform.demoMarkDone")}</Button>}
                <Button variant="quiet" onClick={() => act("admin_demo_delete", r.id)}>
                  {t("common.delete")}
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
