import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Mail } from "lucide-react";
import { api } from "../../lib/api";
import { useInvalidate } from "../../lib/queries";
import { fmtDateTime } from "../../lib/format";
import { Alert, Button, Dialog, Empty, ErrorAlert, Panel, Plaque, Spinner, cx } from "../../components/ui";
import { PrintButton } from "../prints/PrintButton";

interface SentMail {
  id: string;
  to: string;
  subject: string;
  kind: string;
  createdAt: string;
  sentAt: string | null;
  error: string | null;
}
interface EmailResult {
  sent: number;
  withoutEmail: { plotId: string; identifier: string; holderName: string | null }[];
}

/** FR-UGY-01: expiry notices by e-mail; holders without an address get a letter. */
export function EmailNoticeButton({ plotIds, onDone }: { plotIds: string[]; onDone: () => void }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<EmailResult | null>(null);
  const [error, setError] = useState<unknown>(null);
  return (
    <>
      <Button
        icon={<Mail className="size-5" aria-hidden />}
        busy={busy}
        onClick={async () => {
          setBusy(true);
          setError(null);
          try {
            setResult(await api<EmailResult>("notices.email", { plotIds }));
            await invalidate();
          } catch (e) {
            setError(e);
            setResult({ sent: 0, withoutEmail: [] });
          } finally {
            setBusy(false);
          }
        }}
      >
        {t("notify.sendEmail")}
      </Button>
      {result && (
        <Dialog
          open
          onClose={() => (setResult(null), onDone())}
          title={t("notify.emailTitle")}
          footer={<Button onClick={() => (setResult(null), onDone())}>{t("common.close")}</Button>}
        >
          <div className="flex flex-col gap-4">
            <ErrorAlert error={error} />
            {!error && <Alert tone="ok">{t("notify.sent", { count: result.sent })}</Alert>}
            {result.withoutEmail.length > 0 && (
              <>
                <p>{t("notify.withoutEmail", { count: result.withoutEmail.length })}</p>
                <ul className="flex flex-wrap gap-2">
                  {result.withoutEmail.map((w) => (
                    <li key={w.plotId} className="flex items-center gap-2 rounded-md border border-seam px-2 py-1">
                      <Plaque identifier={w.identifier} />
                      <span>{w.holderName ?? t("notify.noHolder")}</span>
                    </li>
                  ))}
                </ul>
                <div>
                  <PrintButton doc="expiry_notice" args={{ plotIds: result.withoutEmail.map((w) => w.plotId) }} label={t("notify.printLetters")} variant="primary" />
                </div>
              </>
            )}
          </div>
        </Dialog>
      )}
    </>
  );
}

/** Delivery log of the e-mails sent on behalf of the operator. */
export function EmailLog() {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["emails"], queryFn: () => api<SentMail[]>("emails.list") });
  const rows = q.data ?? [];
  return (
    <Panel title={t("notify.log")}>
      {!q.data ? (
        <Spinner />
      ) : rows.length === 0 ? (
        <Empty>{t("notify.noMails")}</Empty>
      ) : (
        <table className="w-full min-w-[640px]" data-testid="email-log">
          <thead>
            <tr className="border-b border-seam text-left text-sm text-granite">
              <th className="py-2 pr-4 font-medium">{t("notify.when")}</th>
              <th className="py-2 pr-4 font-medium">{t("notify.to")}</th>
              <th className="py-2 pr-4 font-medium">{t("notify.subject")}</th>
              <th className="py-2 pr-4 font-medium">{t("common.status")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((m) => (
              <tr key={m.id} className="border-b border-seam last:border-0">
                <td className="py-2.5 pr-4 tabular-nums">{fmtDateTime(m.createdAt)}</td>
                <td className="py-2.5 pr-4">{m.to}</td>
                <td className="py-2.5 pr-4">{m.subject}</td>
                <td className={cx("py-2.5 pr-4", m.error && "font-semibold text-alert")}>
                  {m.error ? t("notify.failed") : m.sentAt ? t("notify.delivered") : t("notify.queued")}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Panel>
  );
}

interface ReminderStatus {
  enabled: boolean;
  due: number;
  unanswered: { plotId: string; identifier: string; expiryDate: string; holderName: string | null; email: string | null; phone: string | null; address: string | null; reminders: number; lastNotice: string | null; reason: "no_email" | "no_reply" }[];
}

/** Automatic expiry reminders: switch, and the holders who did not react. */
export function AutoReminders({ canManage }: { canManage: boolean }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const q = useQuery({ queryKey: ["reminders"], queryFn: () => api<ReminderStatus>("reminders.status") });
  const [on, setOn] = useState<boolean | null>(null);
  const [error, setError] = useState<unknown>(null);
  const r = q.data;
  if (!r) return null;
  const enabled = on ?? r.enabled;
  const noEmail = r.unanswered.filter((u) => u.reason === "no_email");
  return (
    <Panel title={t("notify.autoTitle")}>
      <div className="flex flex-col gap-4" data-testid="auto-reminders">
        <p className="text-granite">{t("notify.autoIntro")}</p>
        <div className="flex flex-wrap items-center gap-4">
          <label className="flex cursor-pointer items-center gap-3 font-semibold">
            <input
              type="checkbox"
              className="size-5 accent-[var(--c-moss)]"
              checked={enabled}
              disabled={!canManage}
              onChange={async (e) => {
                const v = e.target.checked;
                setOn(v);
                setError(null);
                try {
                  await api("settings.set", { values: { "notify.auto_expiry": v ? "true" : "false" } });
                  await invalidate();
                } catch (err) {
                  setOn(null);
                  setError(err);
                }
              }}
            />
            {t("notify.autoEnable")}
          </label>
          {enabled && <span className="text-granite">{t("notify.autoDue", { count: r.due })}</span>}
        </div>
        <ErrorAlert error={error} />
        <h3 className="font-semibold">{t("notify.unanswered", { count: r.unanswered.length })}</h3>
        {r.unanswered.length === 0 ? (
          <p className="text-granite">{t("notify.noUnanswered")}</p>
        ) : (
          <>
            <table className="w-full min-w-[720px]" data-testid="unanswered">
              <thead>
                <tr className="border-b border-seam text-left text-sm text-granite">
                  <th className="py-2 pr-4 font-medium">{t("fields.plot")}</th>
                  <th className="py-2 pr-4 font-medium">{t("fields.holder")}</th>
                  <th className="py-2 pr-4 font-medium">{t("fields.leaseExpiry")}</th>
                  <th className="py-2 pr-4 font-medium">{t("notify.whatHappened")}</th>
                </tr>
              </thead>
              <tbody>
                {r.unanswered.map((u) => (
                  <tr key={u.plotId} className="border-b border-seam last:border-0">
                    <td className="py-2.5 pr-4">
                      <Plaque identifier={u.identifier} />
                    </td>
                    <td className="py-2.5 pr-4">
                      <span className="font-semibold">{u.holderName ?? "–"}</span>
                      <span className="block text-sm text-granite">{[u.phone, u.email, u.address].filter(Boolean).join(" · ")}</span>
                    </td>
                    <td className="py-2.5 pr-4 tabular-nums">{u.expiryDate}</td>
                    <td className={cx("py-2.5 pr-4", u.reason === "no_email" && "text-warn")}>
                      {u.reason === "no_email" ? t("notify.reasonNoEmail") : t("notify.reasonNoReply", { count: u.reminders, date: u.lastNotice ?? "" })}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {noEmail.length > 0 && (
              <div>
                <PrintButton doc="expiry_notice" args={{ plotIds: noEmail.map((u) => u.plotId) }} label={t("notify.printForNoEmail", { count: noEmail.length })} />
              </div>
            )}
          </>
        )}
      </div>
    </Panel>
  );
}
