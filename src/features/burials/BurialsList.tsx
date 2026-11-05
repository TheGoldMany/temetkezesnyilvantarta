import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, keepPreviousData } from "@tanstack/react-query";
import { BookPlus } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { EVENT_TYPES, type EventRow, type Page } from "../../lib/types";
import { Button, Checkbox, Empty, Field, Input, PageHeader, Plaque, Select, Spinner, cx , DateInput } from "../../components/ui";
import { fmtDate } from "../../lib/format";
import { useCurrentCemetery } from "../../lib/queries";

const PAGE = 50;

/** The register book on screen (nyilvántartó könyv), newest first. */
export function BurialsList() {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const { current } = useCurrentCemetery();
  const [query, setQuery] = useState("");
  const [type, setType] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [voided, setVoided] = useState(false);
  const [page, setPage] = useState(0);
  const filter = {
    cemeteryId: current?.id ?? null,
    query: query || null,
    eventTypes: type ? [type] : [],
    dateFrom: from || null,
    dateTo: to || null,
    includeVoided: voided,
    limit: PAGE,
    offset: page * PAGE,
    order: "date_desc" as const,
  };
  const { data, isLoading } = useQuery({
    queryKey: ["events", filter],
    queryFn: () => api<Page<EventRow>>("events.list", { filter }),
    placeholderData: keepPreviousData,
  });
  return (
    <>
      <PageHeader
        title={t("burials.title")}
        subtitle={t("burials.subtitle")}
        actions={
          can("record_events") && (
            <Button variant="primary" size="lg" icon={<BookPlus className="size-5" aria-hidden />} onClick={() => nav.go({ view: "newBurial" })}>
              {t("burials.new")}
            </Button>
          )
        }
      />
      <div className="mb-5 grid gap-4 rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)] px-5 py-4 md:grid-cols-[2fr_1.5fr_1fr_1fr]">
        <Field label={t("burials.filterQuery")}>
          <Input value={query} onChange={(e) => (setQuery(e.target.value), setPage(0))} />
        </Field>
        <Field label={t("fields.eventType")}>
          <Select value={type} onChange={(e) => (setType(e.target.value), setPage(0))}>
            <option value="">{t("common.all")}</option>
            {EVENT_TYPES.map((et) => (
              <option key={et} value={et}>
                {t(`eventType.${et}`)}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={`${t("common.date")}, ${t("common.from")}`}>
          <DateInput value={from} onValue={(v) => (setFrom(v), setPage(0))} />
        </Field>
        <Field label={`${t("common.date")}, ${t("common.to")}`}>
          <DateInput value={to} onValue={(v) => (setTo(v), setPage(0))} />
        </Field>
        <div className="md:col-span-4">
          <Checkbox label={t("burials.showVoided")} checked={voided} onChange={setVoided} />
        </div>
      </div>
      {isLoading || !data ? (
        <Spinner />
      ) : data.items.length === 0 ? (
        <Empty>{t("burials.empty")}</Empty>
      ) : (
        <>
          <div className="overflow-x-auto rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)]">
            <table className="w-full border-collapse">
              <thead>
                <tr className="border-b border-seam text-left text-granite">
                  <th className="px-4 py-3 font-semibold">{t("fields.folio")}</th>
                  <th className="px-4 py-3 font-semibold">{t("common.date")}</th>
                  <th className="px-4 py-3 font-semibold">{t("fields.familyName")}, {t("fields.givenName").toLowerCase()}</th>
                  <th className="px-4 py-3 font-semibold">{t("fields.eventType")}</th>
                  <th className="px-4 py-3 font-semibold">{t("fields.plot")}</th>
                </tr>
              </thead>
              <tbody>
                {data.items.map((e) => (
                  <tr
                    key={e.id}
                    tabIndex={0}
                    onClick={() => nav.go({ view: "deceased", id: e.deceasedId })}
                    onKeyDown={(k) => k.key === "Enter" && nav.go({ view: "deceased", id: e.deceasedId })}
                    className={cx("cursor-pointer border-b border-seam last:border-0 hover:bg-limestone", e.voidedAt && "text-granite line-through")}
                  >
                    <td className="px-4 py-3 font-semibold">
                      {e.folio}
                      {e.historical && <span className="ml-1 text-sm font-normal text-granite">(papír)</span>}
                    </td>
                    <td className="px-4 py-3 whitespace-nowrap">{fmtDate(e.eventDate)}</td>
                    <td className="px-4 py-3">
                      <span className="font-semibold">{e.deceasedName || t("fields.unknownIdentity")}</span>
                      {e.birthName && <span className="block text-sm text-granite">szül. {e.birthName}</span>}
                    </td>
                    <td className="px-4 py-3">{t(`eventType.${e.eventType}`)}</td>
                    <td className="px-4 py-3">{e.plotIdentifier ? <Plaque identifier={e.plotIdentifier} /> : <span className="text-granite">{e.destination || "–"}</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="mt-4 flex items-center justify-between gap-3">
            <span className="text-granite">{t("common.page", { from: page * PAGE + 1, to: page * PAGE + data.items.length, total: data.total })}</span>
            <div className="flex gap-2">
              <Button disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
                {t("common.previous")}
              </Button>
              <Button disabled={(page + 1) * PAGE >= data.total} onClick={() => setPage((p) => p + 1)}>
                {t("common.next")}
              </Button>
            </div>
          </div>
        </>
      )}
    </>
  );
}
