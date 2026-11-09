import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { BellRing } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import type { ExpiryRow } from "../../lib/types";
import { Button, Checkbox, Dialog, Empty, ErrorAlert, Field, Input, PageHeader, Plaque, Select, Spinner, StatusBadge, cx , DateInput } from "../../components/ui";
import { fmtDate, todayIso } from "../../lib/format";
import { useCurrentCemetery, useInvalidate } from "../../lib/queries";
import { useToast } from "../../app/toast";
import { PrintButton } from "../prints/PrintButton";
import { AutoReminders, EmailLog, EmailNoticeButton } from "../notify/Email";

/** FR-MEG-05..07, UC-03: expiry watcher with bulk notices. */
export function Expiries() {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const { current } = useCurrentCemetery();
  const [within, setWithin] = useState(365);
  const [includeExpired, setIncludeExpired] = useState(true);
  const [onlyNotNotified, setOnlyNotNotified] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [noticeOpen, setNoticeOpen] = useState(false);
  const filter = { cemeteryId: current?.id ?? null, withinDays: within, includeExpired, onlyNotNotified };
  const { data, isLoading } = useQuery({ queryKey: ["expiries", filter], queryFn: () => api<ExpiryRow[]>("expiries.list", { filter }), enabled: !!current });
  const rows = useMemo(() => data ?? [], [data]);
  const allSelected = rows.length > 0 && rows.every((r) => selected.has(r.plotId));
  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  const ids = [...selected];
  return (
    <>
      <PageHeader title={t("expiries.title")} subtitle={t("expiries.subtitle")} />
      <div className="mb-5 flex flex-wrap items-end gap-5 rounded-xl border border-seam bg-slab px-5 py-4">
        <Field label={t("expiries.within")}>
          <Select value={within} onChange={(e) => setWithin(Number(e.target.value))}>
            <option value={30}>{t("expiries.within30")}</option>
            <option value={90}>{t("expiries.within90")}</option>
            <option value={365}>{t("expiries.within365")}</option>
            <option value={0}>{t("expiries.alreadyExpired")}</option>
          </Select>
        </Field>
        <Checkbox label={t("expiries.includeExpired")} checked={includeExpired || within === 0} onChange={setIncludeExpired} disabled={within === 0} />
        <Checkbox label={t("expiries.onlyNotNotified")} checked={onlyNotNotified} onChange={setOnlyNotNotified} />
      </div>
      {ids.length > 0 && (
        <div className="sticky top-0 z-10 mb-4 flex flex-wrap items-center gap-3 rounded-lg border border-accent bg-lichen px-5 py-3" data-testid="bulk-bar">
          <span className="font-semibold">{t("expiries.selected", { count: ids.length })}</span>
          <PrintButton doc="expiry_notice" args={{ plotIds: ids }} label={t("expiries.printLetters")} />
          <PrintButton doc="grave_labels" args={{ plotIds: ids }} label={t("expiries.printLabels")} />
          <PrintButton doc="notice_board" args={{ plotIds: ids }} label={t("expiries.printBoard")} />
          {can("record_leases") && <EmailNoticeButton plotIds={ids} onDone={() => setSelected(new Set())} />}
          {can("record_leases") && (
            <Button variant="primary" icon={<BellRing className="size-5" aria-hidden />} onClick={() => setNoticeOpen(true)}>
              {t("expiries.recordNotice")}
            </Button>
          )}
        </div>
      )}
      {isLoading ? (
        <Spinner />
      ) : rows.length === 0 ? (
        <Empty>{t("expiries.empty")}</Empty>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-seam bg-slab">
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-seam text-left text-granite">
                <th className="w-12 px-4 py-3">
                  <input type="checkbox" aria-label={t("expiries.selectAll")} className="size-5 accent-[var(--c-slate)]" checked={allSelected} onChange={() => setSelected(allSelected ? new Set() : new Set(rows.map((r) => r.plotId)))} />
                </th>
                <th className="px-4 py-3 font-semibold">{t("fields.plot")}</th>
                <th className="px-4 py-3 font-semibold">{t("fields.leaseExpiry")}</th>
                <th className="px-4 py-3 font-semibold">{t("fields.status")}</th>
                <th className="px-4 py-3 font-semibold">{t("fields.holder")}</th>
                <th className="px-4 py-3 font-semibold">{t("expiries.lastNotice")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.plotId} className={cx("border-b border-seam last:border-0", selected.has(r.plotId) && "bg-lichen/40")}>
                  <td className="px-4 py-3">
                    <input type="checkbox" aria-label={r.identifier} className="size-5 accent-[var(--c-slate)]" checked={selected.has(r.plotId)} onChange={() => toggle(r.plotId)} />
                  </td>
                  <td className="px-4 py-3">
                    <button type="button" onClick={() => nav.go({ view: "plot", id: r.plotId })} className="text-left">
                      <Plaque identifier={r.identifier} />
                      <span className="block truncate text-sm text-granite">{r.occupantNames ?? r.typeName}</span>
                    </button>
                  </td>
                  <td className="px-4 py-3 whitespace-nowrap">
                    {fmtDate(r.leaseExpiry)}
                    <span className={cx("block text-sm", r.daysLeft < 0 ? "text-alert" : "text-granite")}>{r.daysLeft < 0 ? t("expiries.overdueValue", { count: -r.daysLeft }) : t("expiries.daysLeftValue", { count: r.daysLeft })}</span>
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge status={r.status} />
                  </td>
                  <td className="px-4 py-3">
                    {r.holderName ?? "–"}
                    {r.holderAddress && <span className="block text-sm text-granite">{r.holderAddress}</span>}
                  </td>
                  <td className="px-4 py-3">{fmtDate(r.lastNotice)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {can("record_leases") && (
        <div className="mt-8 flex flex-col gap-6">
          <AutoReminders canManage={can("manage_settings")} />
          <EmailLog />
        </div>
      )}
      {noticeOpen && <NoticeDialog plotIds={ids} onClose={(n) => (setNoticeOpen(false), n && setSelected(new Set()))} />}
    </>
  );
}

function NoticeDialog({ plotIds, onClose }: { plotIds: string[]; onClose: (done: boolean) => void }) {
  const { t } = useTranslation();
  const [kind, setKind] = useState("letter");
  const [date, setDate] = useState(todayIso());
  const [result, setResult] = useState("");
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  const toast = useToast();
  return (
    <Dialog
      open
      onClose={() => onClose(false)}
      title={t("expiries.recordNotice")}
      footer={
        <>
          <Button onClick={() => onClose(false)}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              try {
                const n = await api<number>("notices.record", { plotIds, kind, noticeDate: date, result });
                await invalidate();
                toast(t("expiries.noticeRecorded", { count: n }));
                onClose(true);
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("expiries.recordNotice")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <p>{t("expiries.selected", { count: plotIds.length })}</p>
        <Field label={t("expiries.noticeKind")}>
          <Select value={kind} onChange={(e) => setKind(e.target.value)}>
            {["letter", "label", "board", "email"].map((k) => (
              <option key={k} value={k}>
                {t(`noticeKind.${k}`)}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("expiries.noticeDate")}>
          <DateInput value={date} onValue={(v) => setDate(v)} />
        </Field>
        <Field label={t("expiries.noticeResult")}>
          <Input value={result} onChange={(e) => setResult(e.target.value)} placeholder="pl. postára adva, ajánlott levél" />
        </Field>
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}
