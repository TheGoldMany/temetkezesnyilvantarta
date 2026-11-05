import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, keepPreviousData } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { PLOT_STATUSES, type Page, type PlotSummary } from "../../lib/types";
import { Checkbox, Empty, Field, Input, PageHeader, Plaque, Select, Spinner, StatusBadge, cx , DateInput } from "../../components/ui";
import { fmtDate, fmtNum } from "../../lib/format";
import { useCurrentCemetery, usePlotTypes, useSections } from "../../lib/queries";

/** FR-KER-02: compound filtering of plots; virtualised list for large cemeteries. */
export function PlotsList({ sectionId: presetSection }: { sectionId?: string }) {
  const { t } = useTranslation();
  const { nav } = useApp();
  const { current } = useCurrentCemetery();
  const sections = useSections(current?.id);
  const types = usePlotTypes();
  const [sectionId, setSectionId] = useState(presetSection ?? "");
  const [typeId, setTypeId] = useState("");
  const [statuses, setStatuses] = useState<string[]>([]);
  const [ident, setIdent] = useState("");
  const [expFrom, setExpFrom] = useState("");
  const [expTo, setExpTo] = useState("");
  const [burFrom, setBurFrom] = useState("");
  const [burTo, setBurTo] = useState("");
  const [protectedOnly, setProtectedOnly] = useState(false);
  const filter = {
    cemeteryId: current?.id ?? null,
    sectionId: sectionId || null,
    plotTypeId: typeId || null,
    statuses,
    identifierPrefix: ident || null,
    expiryFrom: expFrom || null,
    expiryTo: expTo || null,
    burialFrom: burFrom || null,
    burialTo: burTo || null,
    protectedOnly,
  };
  const { data, isLoading } = useQuery({
    queryKey: ["plots", filter],
    queryFn: () => api<Page<PlotSummary>>("plots.list", { filter }),
    enabled: !!current,
    placeholderData: keepPreviousData,
  });
  const parentRef = useRef<HTMLDivElement>(null);
  const items = data?.items ?? [];
  const virt = useVirtualizer({ count: items.length, getScrollElement: () => parentRef.current, estimateSize: () => 58, overscan: 12 });
  const toggle = (s: string) => setStatuses((cur) => (cur.includes(s) ? cur.filter((x) => x !== s) : [...cur, s]));

  return (
    <>
      <PageHeader title={t("plots.title")} subtitle={data ? t("common.results", { count: data.total }) : undefined} />
      <div className="mb-5 flex flex-col gap-4 rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)] px-5 py-4">
        <div className="grid gap-4 md:grid-cols-4">
          <Field label={t("fields.plotIdentifier")}>
            <Input value={ident} onChange={(e) => setIdent(e.target.value.toUpperCase())} placeholder="A/3" />
          </Field>
          <Field label={t("plots.section")}>
            <Select value={sectionId} onChange={(e) => setSectionId(e.target.value)}>
              <option value="">{t("common.all")}</option>
              {(sections.data ?? []).map((s) => (
                <option key={s.id} value={s.id}>
                  {s.code} {s.name && `– ${s.name}`}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={t("fields.plotType")}>
            <Select value={typeId} onChange={(e) => setTypeId(e.target.value)}>
              <option value="">{t("common.all")}</option>
              {(types.data ?? []).map((pt) => (
                <option key={pt.id} value={pt.id}>
                  {pt.name}
                </option>
              ))}
            </Select>
          </Field>
          <div className="flex items-end pb-2">
            <Checkbox label={t("plots.protected")} checked={protectedOnly} onChange={setProtectedOnly} />
          </div>
          <Field label={`${t("fields.leaseExpiry")}, ${t("common.from")}`}>
            <DateInput value={expFrom} onValue={(v) => setExpFrom(v)} />
          </Field>
          <Field label={`${t("fields.leaseExpiry")}, ${t("common.to")}`}>
            <DateInput value={expTo} onValue={(v) => setExpTo(v)} />
          </Field>
          <Field label={`${t("fields.eventDate")}, ${t("common.from")}`}>
            <DateInput value={burFrom} onValue={(v) => setBurFrom(v)} />
          </Field>
          <Field label={`${t("fields.eventDate")}, ${t("common.to")}`}>
            <DateInput value={burTo} onValue={(v) => setBurTo(v)} />
          </Field>
        </div>
        <fieldset className="flex flex-wrap gap-2">
          <legend className="mb-2 font-semibold">{t("fields.status")}</legend>
          {PLOT_STATUSES.map((s) => (
            <button
              key={s}
              type="button"
              aria-pressed={statuses.includes(s)}
              onClick={() => toggle(s)}
              className={cx("rounded-full border px-3 py-1", statuses.includes(s) ? "border-slate bg-lichen" : "border-seam bg-slab text-granite hover:text-slate")}
            >
              <StatusBadge status={s} />
            </button>
          ))}
        </fieldset>
      </div>
      {isLoading ? (
        <Spinner />
      ) : items.length === 0 ? (
        <Empty>{t("plots.empty")}</Empty>
      ) : (
        <div className="rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)]">
          <div className="grid grid-cols-[9rem_1fr_9rem_10rem_1fr] gap-4 border-b border-seam px-4 py-3 font-semibold text-granite">
            <span>{t("fields.plot")}</span>
            <span>{t("plots.occupants")}</span>
            <span>{t("fields.status")}</span>
            <span>{t("plots.expiry")}</span>
            <span>{t("plots.holder")}</span>
          </div>
          <div ref={parentRef} className="scroll-thin h-[60vh] overflow-y-auto" role="list" aria-label={t("plots.title")}>
            <div style={{ height: virt.getTotalSize(), position: "relative" }}>
              {virt.getVirtualItems().map((v) => {
                const p = items[v.index];
                return (
                  <button
                    key={p.id}
                    type="button"
                    role="listitem"
                    onClick={() => nav.go({ view: "plot", id: p.id })}
                    style={{ position: "absolute", top: 0, left: 0, right: 0, height: v.size, transform: `translateY(${v.start}px)` }}
                    className="grid grid-cols-[9rem_1fr_9rem_10rem_1fr] items-center gap-4 border-b border-seam px-4 text-left hover:bg-limestone"
                  >
                    <span>
                      <Plaque identifier={p.identifier} />
                    </span>
                    <span className="truncate">{p.occupantNames ?? <span className="text-granite">{p.typeName}</span>}</span>
                    <StatusBadge status={p.status} protectedPlot={p.protected} />
                    <span>{fmtDate(p.leaseExpiry)}</span>
                    <span className="truncate">{p.holderName ?? "–"}</span>
                  </button>
                );
              })}
            </div>
          </div>
          <p className="px-4 py-2 text-sm text-granite">{fmtNum(items.length)}</p>
        </div>
      )}
    </>
  );
}
