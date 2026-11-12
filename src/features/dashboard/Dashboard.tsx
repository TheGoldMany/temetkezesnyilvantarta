import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { BookPlus, Check, HardDriveDownload, ChevronRight } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { PLOT_STATUSES, type DashboardStats, type PlotStatus } from "../../lib/types";
import { Button, Empty, PageHeader, Panel, Plaque, Spinner, StatusDot, statusColor, cx } from "../../components/ui";
import { fmtDate, fmtNum } from "../../lib/format";
import { useCurrentCemetery } from "../../lib/queries";
import { useToast } from "../../app/toast";

/** Work surface: what needs doing today, the latest register entries, plot states. */
export function Dashboard() {
  const { t } = useTranslation();
  const { status, nav, can, has, refresh, cemeteryLocked } = useApp();
  const { current, isLoading: cemeteriesLoading } = useCurrentCemetery();
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

  const counts = new Map<PlotStatus, number>((data?.byStatus ?? []) as [PlotStatus, number][]);
  const notified = counts.get("notified") ?? 0;
  const tasks: { key: string; count: number | null; text: string; action: string; go: () => void; urgent?: boolean }[] = [];
  if (status?.backupWarning && can("backup")) {
    tasks.push({ key: "backup", count: null, text: t("dashboard.backupWarning"), action: t("dashboard.backupNow"), go: backupNow, urgent: true });
  }
  if (data && data.expired - notified > 0) {
    tasks.push({ key: "expired", count: data.expired - notified, text: t("dashboard.taskExpired"), action: t("dashboard.checkExpiries"), go: () => nav.go({ view: "expiries" }), urgent: true });
  }
  if (data && data.expiring90 > 0) {
    tasks.push({ key: "soon", count: data.expiring90, text: t("dashboard.taskExpiring"), action: t("dashboard.checkExpiries"), go: () => nav.go({ view: "expiries" }) });
  }
  if (notified > 0) {
    tasks.push({ key: "notified", count: notified, text: t("dashboard.taskNotified"), action: t("dashboard.checkExpiries"), go: () => nav.go({ view: "expiries" }) });
  }
  // 145/1999. Korm. r. 41. § (3): a certified copy of the electronic register (once there is something in it)
  if (data && data.deceasedTotal > 0 && !cemeteryLocked && can("print") && (data.daysSinceCertifiedCopy === null || data.daysSinceCertifiedCopy === undefined || data.daysSinceCertifiedCopy > 31)) {
    tasks.push({
      key: "certified",
      count: null,
      text: data.daysSinceCertifiedCopy == null ? t("dashboard.taskCertifiedNever") : t("dashboard.taskCertified", { days: data.daysSinceCertifiedCopy }),
      action: t("dashboard.taskCertifiedAction"),
      go: () => nav.go({ view: "prints" }),
    });
  }

  return (
    <>
      <PageHeader
        title={t("dashboard.title")}
        subtitle={current?.name}
        actions={
          can("record_events") &&
          current && (
            <Button variant="primary" size="lg" icon={<BookPlus className="size-5" aria-hidden />} onClick={() => nav.go({ view: "newBurial" })} data-tour="new-burial">
              {t("dashboard.newBurial")}
            </Button>
          )
        }
      />
      {isLoading || !data ? (
        <Spinner />
      ) : (
        <div className="flex flex-col gap-9">
          {!cemeteriesLoading && (has("manage_structure") || has("manage_settings")) && (!current || cemeteryLocked || data.plotsTotal === 0) && (
            <FirstSteps hasCemetery={!!current} subscribed={!!current && !cemeteryLocked} hasPlots={data.plotsTotal > 0} hasEntries={data.deceasedTotal > 0} />
          )}
          <section aria-labelledby="tasks-h">
            <h2 id="tasks-h" className="mb-2 text-lg">
              {t("dashboard.tasks")}
            </h2>
            {tasks.length === 0 ? (
              <p className="rounded-xl border border-seam bg-slab px-5 py-4 text-granite">{t("dashboard.noTasks")}</p>
            ) : (
              <ul className="overflow-hidden rounded-xl border border-seam bg-slab">
                {tasks.map((task) => (
                  <li key={task.key} className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-seam px-4 py-3 last:border-0 sm:gap-x-5 sm:px-5">
                    <span className={cx("w-1.5 self-stretch", task.urgent ? "bg-accent" : "bg-seam")} aria-hidden />
                    {task.count !== null && <span className="min-w-14 text-2xl font-extrabold tabular-nums">{fmtNum(task.count)}</span>}
                    <span className="min-w-[11rem] flex-1">{task.text}</span>
                    <Button onClick={task.go} icon={task.key === "backup" ? <HardDriveDownload className="size-5" aria-hidden /> : undefined}>
                      {task.action}
                      {task.key !== "backup" && <ChevronRight className="size-4" aria-hidden />}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <div className="grid gap-9 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
            <Panel
              title={t("dashboard.recent")}
              actions={
                <Button variant="quiet" onClick={() => nav.go({ view: "burials" })}>
                  {t("dashboard.allEntries")}
                </Button>
              }
            >
              {data.recentEvents.length === 0 ? (
                <Empty>{t("dashboard.noEvents")}</Empty>
              ) : (
                <table className="-mx-5 -my-4 w-[calc(100%+2.5rem)]">
                  <thead>
                    <tr className="border-b border-seam text-left text-sm text-granite">
                      <th className="px-5 py-2 font-bold">{t("fields.folio")}</th>
                      <th className="py-2 pr-4 font-bold">{t("common.date")}</th>
                      <th className="py-2 pr-4 font-bold">{t("dashboard.colName")}</th>
                      <th className="py-2 pr-5 text-right font-bold">{t("fields.plot")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.recentEvents.map((e) => {
                      const open = () => (e.plotId ? nav.go({ view: "plot", id: e.plotId }) : nav.go({ view: "deceased", id: e.deceasedId }));
                      return (
                        <tr key={e.id} tabIndex={0} onClick={open} onKeyDown={(k) => k.key === "Enter" && open()} className="cursor-pointer border-b border-seam last:border-0 hover:bg-limestone">
                          <td className="px-5 py-2.5 font-bold tabular-nums">{e.folio}.</td>
                          <td className="py-2.5 pr-4 whitespace-nowrap tabular-nums">{fmtDate(e.eventDate)}</td>
                          <td className="py-2.5 pr-4">
                            <span className="block font-bold">{e.deceasedName || t("fields.unknownIdentity")}</span>
                            <span className="block text-sm text-granite">{t(`eventType.${e.eventType}`)}</span>
                          </td>
                          <td className="py-2.5 pr-5 text-right">
                            <Plaque identifier={e.plotIdentifier} />
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              )}
            </Panel>

            <Panel title={t("dashboard.occupancy")}>
              <StatusBar counts={counts} total={data.plotsTotal} />
              <table className="mt-4 w-full">
                <tbody>
                  {PLOT_STATUSES.filter((s) => (counts.get(s) ?? 0) > 0).map((s) => (
                    <tr key={s} className="border-b border-seam last:border-0">
                      <td className="py-1.5">
                        <span className="flex items-center gap-2">
                          <StatusDot status={s} />
                          {t(`status.${s}`)}
                        </span>
                      </td>
                      <td className="py-1.5 text-right font-bold tabular-nums">{fmtNum(counts.get(s) ?? 0)}</td>
                      <td className="w-16 py-1.5 text-right text-sm text-granite tabular-nums">{Math.round(((counts.get(s) ?? 0) / Math.max(data.plotsTotal, 1)) * 100)}%</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <dl className="mt-4 grid grid-cols-2 gap-x-4 gap-y-1 border-t border-seam pt-3">
                <dt className="text-granite">{t("dashboard.plotsTotal")}</dt>
                <dd className="m-0 text-right font-bold tabular-nums">{fmtNum(data.plotsTotal)}</dd>
                <dt className="text-granite">{t("dashboard.eventsThisYear")}</dt>
                <dd className="m-0 text-right font-bold tabular-nums">{fmtNum(data.eventsThisYear)}</dd>
                <dt className="text-granite">{t("dashboard.deceasedTotal")}</dt>
                <dd className="m-0 text-right font-bold tabular-nums">{fmtNum(data.deceasedTotal)}</dd>
              </dl>
            </Panel>
          </div>
        </div>
      )}
    </>
  );
}

function StatusBar({ counts, total }: { counts: Map<PlotStatus, number>; total: number }) {
  const { t } = useTranslation();
  const label = PLOT_STATUSES.filter((s) => counts.get(s))
    .map((s) => `${t(`status.${s}`)}: ${counts.get(s)}`)
    .join(", ");
  return (
    <div className="flex h-5 w-full overflow-hidden rounded-md border border-slate/40" role="img" aria-label={label}>
      {PLOT_STATUSES.map((s) => {
        const n = counts.get(s) ?? 0;
        if (!n) return null;
        return (
          <span
            key={s}
            className={cx(s === "notified" || s === "unusable" ? "pattern-hatch" : "", "h-full border-r border-slab last:border-0")}
            style={{ width: `${(n / Math.max(total, 1)) * 100}%`, background: statusColor[s] }}
          />
        );
      })}
    </div>
  );
}

/** A new organisation's way to a working register, shown until the cemetery has plots. */
function FirstSteps({ hasCemetery, subscribed, hasPlots, hasEntries }: { hasCemetery: boolean; subscribed: boolean; hasPlots: boolean; hasEntries: boolean }) {
  const { t } = useTranslation();
  const { nav, has, status } = useApp();
  const steps: { key: string; done: boolean; go?: () => void; action?: string }[] = [
    { key: "cemetery", done: hasCemetery, go: has("manage_structure") ? () => nav.go({ view: "settings", tab: "cemeteries" }) : undefined, action: t("firstSteps.cemeteryAction") },
    { key: "subscription", done: subscribed, go: hasCemetery && has("manage_settings") ? () => nav.go({ view: "settings", tab: "subscription" }) : undefined, action: t("firstSteps.subscriptionAction") },
    { key: "plots", done: hasPlots, go: subscribed && has("manage_structure") ? () => nav.go({ view: "settings", tab: "cemeteries" }) : undefined, action: t("firstSteps.plotsAction") },
    { key: "entries", done: hasEntries, go: subscribed && hasPlots && has("import") ? () => nav.go({ view: "importExport" }) : undefined, action: t("firstSteps.entriesAction") },
  ];
  const next = steps.find((x) => !x.done);
  return (
    <section aria-labelledby="first-h" className="border border-seam border-l-[6px] border-l-accent bg-slab px-5 py-4" data-testid="first-steps">
      <h2 id="first-h" className="text-lg">
        {t("firstSteps.title")}
      </h2>
      <p className="mb-3 text-granite">{status?.account?.demoEndsAt ? t("firstSteps.introDemo", { date: status.account.demoEndsAt }) : t("firstSteps.intro")}</p>
      <ol className="flex flex-col divide-y divide-seam border-y border-seam">
        {steps.map((x, i) => (
          <li key={x.key} className="flex flex-wrap items-center gap-x-4 gap-y-2 py-2.5">
            <span className={cx("flex size-7 shrink-0 items-center justify-center text-sm font-semibold", x.done ? "bg-moss text-white" : "border border-granite text-granite")} aria-hidden>
              {x.done ? <Check className="size-4" /> : i + 1}
            </span>
            <span className={cx("min-w-[12rem] flex-1", x.done && "text-granite line-through")}>
              <span className="sr-only">{x.done ? t("firstSteps.done") : ""} </span>
              {t(`firstSteps.${x.key}`)}
            </span>
            {x === next && x.go && (
              <Button variant="primary" onClick={x.go}>
                {x.action}
                <ChevronRight className="size-4" aria-hidden />
              </Button>
            )}
          </li>
        ))}
      </ol>
      <p className="mt-3 text-sm text-granite">
        {t("firstSteps.help")}{" "}
        <a className="font-semibold text-moss underline underline-offset-3" href="/bemutato" target="_blank" rel="noreferrer">
          {t("firstSteps.helpLink")}
        </a>
      </p>
    </section>
  );
}
