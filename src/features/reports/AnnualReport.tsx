import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { fmtHuf, fmtNum } from "../../lib/format";
import { Field, Panel, Select, Spinner, cx } from "../../components/ui";
import { ExportButton } from "../io/ExportButton";

interface Capacity {
  sectionId: string;
  cemeteryName: string;
  sectionCode: string;
  sectionName: string;
  total: number;
  free: number;
  resellable: number;
  expired: number;
  perYear: number;
  yearsLeft: number | null;
}
interface Annual {
  year: number;
  burials: { eventType: string; n: number }[];
  burialsTotal: number;
  leases: { kind: string; n: number; feeHuf: number }[];
  revenue: { feeKind: string; ledgerCode: string; netHuf: number; vatHuf: number; grossHuf: number }[];
  revenueGrossHuf: number;
  reversedHuf: number;
  paymentsHuf: number;
  receivables: { count: number; openHuf: number; overdueHuf: number };
  cash: { inHuf: number; outHuf: number };
  plots: { total: number; byStatus: Record<string, number> };
  workDone: { kind: string; n: number }[];
  ceremonies: number;
  cases: { kind: string; n: number; approved: number; rejected: number; pending: number }[];
  capacity: Capacity[];
}

const th = "py-2 pr-4 text-left text-sm font-medium text-granite";
const td = "py-2 pr-4";

function Figure({ label, value, tone }: { label: string; value: string; tone?: "warn" }) {
  return (
    <div className="min-w-0 border-l-2 border-moss/40 pl-3">
      <p className="text-xs font-semibold uppercase tracking-wide text-granite">{label}</p>
      <p className={cx("text-2xl font-semibold tabular-nums", tone === "warn" && "text-warn")}>{value}</p>
    </div>
  );
}

function Mini({ title, rows, empty }: { title: string; rows: [string, string][]; empty: string }) {
  return (
    <section className="min-w-0">
      <h3 className="mb-1 font-semibold">{title}</h3>
      {rows.length === 0 ? (
        <p className="text-granite">{empty}</p>
      ) : (
        <dl className="grid grid-cols-[1fr_auto] gap-x-4">
          {rows.map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="border-b border-seam py-1.5">{k}</dt>
              <dd className="border-b border-seam py-1.5 text-right tabular-nums">{v}</dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}

/** FR-FEN-01 / FR-FEN-02: annual report for the maintainer with the capacity forecast. */
export function AnnualReport({ cemeteryId }: { cemeteryId: string | null }) {
  const { t } = useTranslation();
  const thisYear = new Date().getFullYear();
  const [year, setYear] = useState(thisYear - 1);
  const q = useQuery({ queryKey: ["annual", cemeteryId, year], queryFn: () => api<Annual>("reports.annual", { cemeteryId, year }) });
  const r = q.data;
  const none = t("annual.none");
  return (
    <Panel
      title={t("annual.title")}
      actions={
        <>
          <Field label={t("reports.year")}>
            <Select value={year} onChange={(e) => setYear(Number(e.target.value))} name="annualYear">
              {Array.from({ length: 8 }, (_, i) => thisYear - i).map((y) => (
                <option key={y} value={y}>
                  {y}
                </option>
              ))}
            </Select>
          </Field>
          <div className="self-end">
            <ExportButton kind="annual" args={{ cemeteryId, year }} label={t("reports.exportXlsx")} />
          </div>
        </>
      }
    >
      {!r ? (
        <Spinner />
      ) : (
        <div className="flex flex-col gap-8" data-testid="annual-report">
          <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-4">
            <Figure label={t("annual.burials")} value={fmtNum(r.burialsTotal)} />
            <Figure label={t("annual.revenue")} value={fmtHuf(r.revenueGrossHuf)} />
            <Figure label={t("annual.payments")} value={fmtHuf(r.paymentsHuf)} />
            <Figure label={t("annual.receivables")} value={fmtHuf(r.receivables.openHuf)} tone={r.receivables.overdueHuf > 0 ? "warn" : undefined} />
          </div>
          <div className="grid gap-8 lg:grid-cols-3">
            <Mini title={t("annual.burialsByType")} empty={none} rows={r.burials.map((b) => [t(`eventType.${b.eventType}`), fmtNum(b.n)])} />
            <Mini
              title={t("annual.leases")}
              empty={none}
              rows={r.leases.map((l) => [t(`annual.leaseKinds.${l.kind}`, { defaultValue: l.kind }), `${fmtNum(l.n)} db · ${fmtHuf(l.feeHuf)}`])}
            />
            <Mini
              title={t("annual.revenueByFee")}
              empty={none}
              rows={r.revenue.map((x) => [
                t(`settings.feeKinds.${x.feeKind}`, { defaultValue: x.feeKind === "other" ? t("annual.otherFee") : x.feeKind }) + (x.ledgerCode ? ` (${x.ledgerCode})` : ""),
                fmtHuf(x.grossHuf),
              ])}
            />
            <Mini
              title={t("annual.plots")}
              empty={none}
              rows={[[t("reports.total"), fmtNum(r.plots.total)], ...Object.entries(r.plots.byStatus).map(([k, v]) => [t(`status.${k}`), fmtNum(v)] as [string, string])]}
            />
            <Mini
              title={t("annual.work")}
              empty={none}
              rows={[[t("annual.ceremonies"), fmtNum(r.ceremonies)], ...r.workDone.map((w) => [t(`ops.orderKinds.${w.kind}`), fmtNum(w.n)] as [string, string])]}
            />
            <Mini
              title={t("annual.cases")}
              empty={none}
              rows={r.cases.map((c) => [t(`ops.caseKinds.${c.kind}`), t("annual.caseCounts", { n: c.n, approved: c.approved, pending: c.pending })])}
            />
          </div>
          <p className="text-sm text-granite">
            {t("annual.moneyNote", { reversed: fmtHuf(r.reversedHuf), cashIn: fmtHuf(r.cash.inHuf), cashOut: fmtHuf(r.cash.outHuf), overdue: fmtHuf(r.receivables.overdueHuf) })}
          </p>
          <section>
            <h3 className="mb-1 font-semibold">{t("annual.capacity")}</h3>
            <p className="mb-3 text-sm text-granite">{t("annual.capacityHint")}</p>
            <div className="overflow-x-auto">
              <table className="w-full min-w-[640px]" data-testid="capacity">
                <thead>
                  <tr className="border-b border-seam">
                    <th className={th}>{t("plots.section")}</th>
                    <th className={cx(th, "text-right")}>{t("reports.total")}</th>
                    <th className={cx(th, "text-right")}>{t("status.free")}</th>
                    <th className={cx(th, "text-right")}>{t("status.resellable")}</th>
                    <th className={cx(th, "text-right")}>{t("annual.expired")}</th>
                    <th className={cx(th, "text-right")}>{t("annual.perYear")}</th>
                    <th className={cx(th, "text-right")}>{t("annual.yearsLeft")}</th>
                  </tr>
                </thead>
                <tbody>
                  {r.capacity.map((c) => (
                    <tr key={c.sectionId} className="border-b border-seam last:border-0">
                      <td className={cx(td, "font-semibold")}>
                        {c.sectionCode}
                        {c.sectionName && <span className="font-normal text-granite"> · {c.sectionName}</span>}
                      </td>
                      <td className={cx(td, "text-right tabular-nums")}>{fmtNum(c.total)}</td>
                      <td className={cx(td, "text-right tabular-nums")}>{fmtNum(c.free)}</td>
                      <td className={cx(td, "text-right tabular-nums")}>{fmtNum(c.resellable)}</td>
                      <td className={cx(td, "text-right tabular-nums")}>{fmtNum(c.expired)}</td>
                      <td className={cx(td, "text-right tabular-nums")}>{c.perYear.toLocaleString("hu-HU")}</td>
                      <td className={cx(td, "text-right font-semibold tabular-nums", c.yearsLeft !== null && c.yearsLeft < 3 && "text-warn")}>
                        {c.yearsLeft === null ? "–" : t("annual.years", { n: c.yearsLeft.toLocaleString("hu-HU") })}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      )}
    </Panel>
  );
}
