import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useInfiniteQuery } from "@tanstack/react-query";
import { ShieldCheck } from "lucide-react";
import { api } from "../../lib/api";
import type { AuditEntry } from "../../lib/types";
import { Alert, Button, Panel, Spinner } from "../../components/ui";
import { fmtDateTime } from "../../lib/format";

/** FR-NAP-01: browse and verify the hash-chained change log. */
export function AuditTab() {
  const { t } = useTranslation();
  const [chain, setChain] = useState<{ ok: boolean; entries: number; brokenAt: number | null } | null>(null);
  const q = useInfiniteQuery({
    queryKey: ["audit"],
    queryFn: ({ pageParam }) => api<AuditEntry[]>("audit.list", { limit: 100, beforeSeq: pageParam }),
    initialPageParam: null as number | null,
    getNextPageParam: (last) => (last.length === 100 ? last[last.length - 1].seq : undefined),
  });
  const rows = q.data?.pages.flat() ?? [];
  return (
    <div className="flex flex-col gap-6">
      <Panel
        title={t("audit.title")}
        actions={
          <Button icon={<ShieldCheck className="size-5" aria-hidden />} onClick={async () => setChain(await api("audit.verify"))}>
            {t("audit.verify")}
          </Button>
        }
      >
        {chain && (
          <div className="mb-4">{chain.ok ? <Alert tone="ok">{t("audit.chainOk", { count: chain.entries })}</Alert> : <Alert tone="error">{t("audit.chainBroken", { seq: chain.brokenAt })}</Alert>}</div>
        )}
        {q.isLoading ? (
          <Spinner />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-granite">
                  <th className="py-2 pr-3">#</th>
                  <th className="py-2 pr-3">{t("audit.when")}</th>
                  <th className="py-2 pr-3">{t("audit.who")}</th>
                  <th className="py-2 pr-3">{t("audit.what")}</th>
                  <th className="py-2 pr-3">{t("audit.entity")}</th>
                  <th className="py-2">{t("common.reason")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.seq} className="border-t border-seam align-top">
                    <td className="py-2 pr-3 text-granite">{r.seq}</td>
                    <td className="py-2 pr-3 whitespace-nowrap">{fmtDateTime(r.ts)}</td>
                    <td className="py-2 pr-3">{r.userName}</td>
                    <td className="py-2 pr-3">{t(`audit.actions.${r.action}`, { defaultValue: r.action })}</td>
                    <td className="py-2 pr-3">
                      {t(`audit.entities.${r.entity}`, { defaultValue: r.entity })}
                      <details>
                        <summary className="cursor-pointer text-granite">{t("common.details")}</summary>
                        {r.oldValue && <pre className="max-w-xl overflow-x-auto whitespace-pre-wrap text-xs">− {r.oldValue}</pre>}
                        {r.newValue && <pre className="max-w-xl overflow-x-auto whitespace-pre-wrap text-xs">+ {r.newValue}</pre>}
                        <p className="font-mono text-xs text-granite">{r.hash}</p>
                      </details>
                    </td>
                    <td className="py-2">{r.reason ?? ""}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {q.hasNextPage && (
          <Button className="mt-3" onClick={() => q.fetchNextPage()}>
            {t("audit.loadMore")}
          </Button>
        )}
      </Panel>
    </div>
  );
}
