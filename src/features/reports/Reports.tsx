import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { EVENT_TYPES, PLOT_STATUSES } from "../../lib/types";
import { PageHeader, Panel, Spinner, StatusDot } from "../../components/ui";
import { fmtNum } from "../../lib/format";
import { useCurrentCemetery } from "../../lib/queries";
import { PrintButton } from "../prints/PrintButton";
import { ExportButton } from "../io/ExportButton";
import { ToolsTabs } from "../prints/ToolsTabs";
import { AnnualReport } from "./AnnualReport";

interface UtilRow {
  cemeteryName: string;
  sectionCode: string;
  total: number;
  byStatus: Record<string, number>;
}
interface YearRow {
  year: number;
  byType: Record<string, number>;
  total: number;
}

/** FR-RIP-01, FR-RIP-02, FR-RIP-05 */
export function Reports() {
  const { t } = useTranslation();
  const { current } = useCurrentCemetery();
  const util = useQuery({ queryKey: ["util", current?.id], queryFn: () => api<UtilRow[]>("reports.utilization", { cemeteryId: current?.id ?? null }) });
  const yearly = useQuery({ queryKey: ["yearly", current?.id], queryFn: () => api<YearRow[]>("reports.yearly", { cemeteryId: current?.id ?? null }) });
  const args = { cemeteryId: current?.id ?? null };
  return (
    <>
      <PageHeader title={t("nav.printsReports")} subtitle={current?.name} />
      <ToolsTabs value="reports" />
      <div className="flex flex-col gap-6">
        <AnnualReport cemeteryId={current?.id ?? null} />
        <Panel
          title={t("reports.utilization")}
          actions={
            <>
              <ExportButton kind="report_utilization" args={args} label={t("reports.exportXlsx")} />
              <PrintButton doc="report_utilization" args={args} label={t("reports.exportPdf")} />
            </>
          }
        >
          {!util.data ? (
            <Spinner />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="text-left text-granite">
                    <th className="py-2 pr-4">{t("plots.section")}</th>
                    <th className="py-2 pr-4">{t("reports.total")}</th>
                    {PLOT_STATUSES.map((s) => (
                      <th key={s} className="py-2 pr-4 font-normal">
                        <span className="inline-flex items-center gap-1">
                          <StatusDot status={s} />
                          {t(`status.${s}`)}
                        </span>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {util.data.map((r) => (
                    <tr key={r.cemeteryName + r.sectionCode} className="border-t border-seam">
                      <td className="py-2 pr-4 font-semibold">{r.sectionCode}</td>
                      <td className="py-2 pr-4 font-semibold">{fmtNum(r.total)}</td>
                      {PLOT_STATUSES.map((s) => (
                        <td key={s} className="py-2 pr-4">
                          {r.byStatus[s] ? fmtNum(r.byStatus[s]) : "–"}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Panel>
        <Panel
          title={t("reports.yearly")}
          actions={
            <>
              <ExportButton kind="report_yearly" args={args} label={t("reports.exportXlsx")} />
              <PrintButton doc="report_yearly" args={args} label={t("reports.exportPdf")} />
            </>
          }
        >
          {!yearly.data ? (
            <Spinner />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="text-left text-granite">
                    <th className="py-2 pr-4">{t("reports.year")}</th>
                    {EVENT_TYPES.map((e) => (
                      <th key={e} className="py-2 pr-4 font-normal">
                        {t(`eventType.${e}`)}
                      </th>
                    ))}
                    <th className="py-2">{t("reports.total")}</th>
                  </tr>
                </thead>
                <tbody>
                  {yearly.data.map((r) => (
                    <tr key={r.year} className="border-t border-seam">
                      <td className="py-2 pr-4 font-semibold">{r.year}</td>
                      {EVENT_TYPES.map((e) => (
                        <td key={e} className="py-2 pr-4">
                          {r.byType[e] ?? "–"}
                        </td>
                      ))}
                      <td className="py-2 font-semibold">{r.total}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Panel>
      </div>
    </>
  );
}
