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
