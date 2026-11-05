import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { BookPlus, Search, CalendarClock, HardDriveDownload } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { PLOT_STATUSES, type DashboardStats, type PlotStatus } from "../../lib/types";
import { Alert, Button, Empty, PageHeader, Panel, Plaque, Spinner, StatusDot } from "../../components/ui";
import { fmtDate, fmtNum } from "../../lib/format";
import { useCurrentCemetery } from "../../lib/queries";
import { useToast } from "../../app/toast";

export function Dashboard() {
  const { t } = useTranslation();
  const { status, nav, can, refresh } = useApp();
  const { current } = useCurrentCemetery();
  const toast = useToast();
  const { data, isLoading } = useQuery({
    queryKey: ["dashboard", current?.id],
    queryFn: () => api<DashboardStats>("dashboard", { cemeteryId: current?.id ?? null }),
  });
  const backupNow = async () => {
    try {
      const b = await api<{ manifest: { fileName: string } }>("backup.create");
      toast(t("backup.done", { file: b.manifest.fileName }));
      await refresh();
    } catch (e) {
      toast(String((e as Error).message), "error");
    }
  };
  return (
    <>
      <PageHeader
        title={t("dashboard.greeting", { name: status?.session?.displayName?.split(" ").slice(-1)[0] ?? "" })}
        subtitle={current?.name}
        actions={
          can("record_events") && (
            <Button variant="primary" size="lg" icon={<BookPlus className="size-5" aria-hidden />} onClick={() => nav.go({ view: "newBurial" })}>
              {t("dashboard.newBurial")}
            </Button>
          )
        }
      />
      {status?.backupWarning && (
        <div className="mb-6">
          <Alert tone="warn" title={t("dashboard.backupWarning")}>
            {can("backup") && (
              <Button className="mt-2" icon={<HardDriveDownload className="size-5" aria-hidden />} onClick={backupNow}>
                {t("dashboard.backupNow")}
              </Button>
            )}
          </Alert>
        </div>
      )}
      {isLoading || !data ? (
        <Spinner />
      ) : (
        <div className="grid gap-6 lg:grid-cols-[1fr_22rem]">
          <div className="flex flex-col gap-6">
            <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-seam bg-seam md:grid-cols-4">
              {[
                [fmtNum(data.plotsTotal), t("dashboard.plotsTotal"), () => nav.go({ view: "plots" })],
                [fmtNum(data.eventsThisYear), t("dashboard.eventsThisYear"), () => nav.go({ view: "burials" })],
                [fmtNum(data.expiring90), t("dashboard.expiring"), () => nav.go({ view: "expiries" })],
                [fmtNum(data.expired), t("dashboard.expired"), () => nav.go({ view: "expiries" })],
              ].map(([n, label, go], i) => (
                <button key={i} type="button" onClick={go as () => void} className="flex flex-col items-start bg-slab px-5 py-4 text-left hover:bg-limestone">
                  <dd className="m-0 font-serif text-3xl font-semibold">{n as string}</dd>
                  <dt className="text-granite">{label as string}</dt>
                </button>
              ))}
            </dl>
            <Panel title={t("dashboard.recent")}>
              {data.recentEvents.length === 0 ? (
                <Empty>{t("dashboard.noEvents")}</Empty>
              ) : (
                <ul className="divide-y divide-seam">
                  {data.recentEvents.map((e) => (
                    <li key={e.id}>
                      <button type="button" className="flex w-full items-center justify-between gap-4 py-3 text-left hover:bg-limestone/60" onClick={() => (e.plotId ? nav.go({ view: "plot", id: e.plotId }) : nav.go({ view: "deceased", id: e.deceasedId }))}>
                        <span className="min-w-0">
                          <span className="block font-semibold">{e.deceasedName || t("fields.unknownIdentity")}</span>
                          <span className="block text-sm text-granite">
                            {t(`eventType.${e.eventType}`)} · {fmtDate(e.eventDate)} · {t("plots.folioShort", { folio: e.folio })} {t("fields.folio").toLowerCase()}
                          </span>
                        </span>
                        <Plaque identifier={e.plotIdentifier} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </Panel>
          </div>
          <div className="flex flex-col gap-6">
            <Panel title={t("dashboard.quickActions")}>
              <div className="flex flex-col gap-2">
                <Button className="justify-start" icon={<Search className="size-5" aria-hidden />} onClick={() => document.querySelector<HTMLInputElement>("input[type=search]")?.focus()}>
                  {t("dashboard.findGrave")}
                </Button>
                <Button className="justify-start" icon={<CalendarClock className="size-5" aria-hidden />} onClick={() => nav.go({ view: "expiries" })}>
                  {t("dashboard.checkExpiries")}
                </Button>
              </div>
            </Panel>
            <Panel title={t("dashboard.occupancy")}>
              <ul className="flex flex-col gap-2">
                {[...data.byStatus]
                  .sort(([a], [b]) => PLOT_STATUSES.indexOf(a) - PLOT_STATUSES.indexOf(b))
                  .filter(([, n]) => n > 0)
                  .map(([s, n]) => (
                    <li key={s} className="flex items-center justify-between gap-3">
                      <span className="flex items-center gap-2">
                        <StatusDot status={s as PlotStatus} />
                        {t(`status.${s}`)}
                      </span>
                      <span className="font-semibold">{fmtNum(n)}</span>
                    </li>
                  ))}
              </ul>
              <p className="mt-3 border-t border-seam pt-3 text-granite">
                {fmtNum(data.deceasedTotal)} {t("dashboard.deceasedTotal")}
              </p>
            </Panel>
          </div>
        </div>
      )}
    </>
  );
}
