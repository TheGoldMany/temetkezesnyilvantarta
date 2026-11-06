import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Minus, Plus, Maximize2, Pencil, RotateCw, Combine, Save, X } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { PLOT_STATUSES, type Plot, type PlotDetail, type PlotStatus, type Section } from "../../lib/types";
import { Alert, Button, Dialog, ErrorAlert, Input, PageHeader, Plaque, Spinner, StatusBadge, StatusDot, statusColor, useErrorText, cx } from "../../components/ui";
import { ReasonField } from "../../components/forms";
import { fmtDate, fmtNum } from "../../lib/format";
import { useCurrentCemetery, useInvalidate } from "../../lib/queries";
import { useToast } from "../../app/toast";
import { PrintButton } from "../prints/PrintButton";
import { buildGrid, queryGrid, type Box } from "./spatial";

/** Below this scale (px per dm) plots are too small: draw sections only. */
const DETAIL_SCALE = 0.55;
const LABEL_SCALE = 2.2;
const MAX_DRAWN = 6000;

interface View {
  x: number; // world coordinate at the left edge
  y: number; // world coordinate at the top edge
  scale: number; // px per world unit (dm)
}

type Geo = { x: number; y: number; w: number; h: number; rot: number };

/** Compact plot used by the map; keyed by its identifier ("A/3/12"). */
interface MapPlot {
  id: string; // identifier
  identifier: string;
  sectionId: string;
  status: PlotStatus;
  protected: boolean;
  x: number;
  y: number;
  w: number;
  h: number;
  rot: number;
}

interface MapData {
  sections: Section[];
  statuses: PlotStatus[];
  plots: [string, number, number, number, number, number, number, number, number][];
}

async function resolveId(cemeteryId: string, identifier: string): Promise<string> {
  const p = await api<Plot | null>("plots.find", { cemeteryId, identifier });
  if (!p) throw new Error(identifier);
  return p.id;
}

/** FR-TER-01/03, FR-KER-05: SVG cemetery map, virtualised for 50 000 plots. */
export function MapView({ plotId }: { plotId?: string }) {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const { current } = useCurrentCemetery();
  const toast = useToast();
  const errText = useErrorText();
  const invalidate = useInvalidate();
  const mapData = useQuery({
    queryKey: ["mapData", current?.id],
    queryFn: () => api<MapData>("map.data", { cemeteryId: current?.id }),
    enabled: !!current,
    staleTime: 60_000,
  });
  const sections = { data: mapData.data?.sections };
  const plots = {
    isLoading: mapData.isLoading,
    data: useMemo(() => {
      const d = mapData.data;
      if (!d) return undefined;
      const items: MapPlot[] = d.plots.map(([identifier, si, st, prot, x, y, w, h, rot]) => ({
        id: identifier,
        identifier,
        sectionId: d.sections[si]?.id ?? "",
        status: d.statuses[st],
        protected: prot === 1,
        x,
        y,
        w,
        h,
        rot,
      }));
      return { items };
    }, [mapData.data]),
  };
  // the detail page passes a database id: translate it to the identifier
  const focusQuery = useQuery({ queryKey: ["plotRaw", plotId], queryFn: () => api<PlotDetail>("plots.get", { id: plotId }), enabled: !!plotId });
  const focusIdent = focusQuery.data?.plot.identifier;
  const wrapRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 1000, h: 640 });
  const [view, setView] = useState<View>({ x: 0, y: 0, scale: 1 });
  const [selected, setSelected] = useState<string | null>(null);
  const [multi, setMulti] = useState<string[]>([]);
  const [editing, setEditing] = useState(false);
  const [moved, setMoved] = useState<Record<string, Geo>>({});
  const [movedSections, setMovedSections] = useState<Record<string, Geo>>({});
  const [jump, setJump] = useState("");
  const [merging, setMerging] = useState(false);
  const fitted = useRef(false);

  const items = useMemo(() => plots.data?.items ?? [], [plots.data]);
  const geo = useCallback((p: MapPlot): Geo => moved[p.id] ?? { x: p.x, y: p.y, w: p.w, h: p.h, rot: p.rot }, [moved]);
  const grid = useMemo(() => buildGrid(items.map((p) => ({ id: p.id, x: p.x, y: p.y, w: p.w, h: p.h }))), [items]);
  const byId = useMemo(() => new Map(items.map((p) => [p.id, p])), [items]);

  // resize observer
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const bounds = useMemo<Box | null>(() => {
    const secs = sections.data ?? [];
    if (!secs.length && !items.length) return null;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const s of secs) {
      x0 = Math.min(x0, s.mapX); y0 = Math.min(y0, s.mapY);
      x1 = Math.max(x1, s.mapX + s.mapW); y1 = Math.max(y1, s.mapY + s.mapH);
    }
    if (!secs.length) for (const p of items) {
      x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y);
      x1 = Math.max(x1, p.x + p.w); y1 = Math.max(y1, p.y + p.h);
    }
    return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  }, [sections.data, items]);

  const fit = useCallback(() => {
    if (!bounds) return;
    const pad = 40;
    const scale = Math.min((size.w - pad * 2) / Math.max(bounds.w, 1), (size.h - pad * 2) / Math.max(bounds.h, 1));
    setView({ scale, x: bounds.x - (size.w / scale - bounds.w) / 2, y: bounds.y - (size.h / scale - bounds.h) / 2 });
  }, [bounds, size]);

  const focusPlot = useCallback(
    (p: MapPlot) => {
      const scale = Math.max(view.scale, 4);
      setView({ scale, x: p.x + p.w / 2 - size.w / 2 / scale, y: p.y + p.h / 2 - size.h / 2 / scale });
      setSelected(p.id);
    },
    [size, view.scale],
  );

  // initial view: focus on the requested plot (FR-KER-05) or fit the cemetery
  useEffect(() => {
    if (fitted.current || !bounds || !items.length || size.w < 50) return;
    if (plotId && !focusIdent) return; // wait for the identifier
    fitted.current = true;
    const target = focusIdent ? byId.get(focusIdent) : undefined;
    if (target) focusPlot(target);
    else fit();
  }, [bounds, items.length, size.w, plotId, focusIdent, byId, fit, focusPlot]);

  const zoomAt = (factor: number, cx = size.w / 2, cy = size.h / 2) => {
    setView((v) => {
      const scale = Math.min(60, Math.max(0.02, v.scale * factor));
      const wx = v.x + cx / v.scale;
      const wy = v.y + cy / v.scale;
      return { scale, x: wx - cx / scale, y: wy - cy / scale };
    });
  };

  // ---- pointer handling: pan, or drag items in edit mode
  const drag = useRef<null | { kind: "pan" | "plot" | "section"; id?: string; sx: number; sy: number; vx: number; vy: number; orig?: Geo; children?: [string, Geo][] }>(null);
  const onPointerDown = (e: RPointerEvent<SVGSVGElement>) => {
    const target = (e.target as Element).closest("[data-plot],[data-section]");
    const plotIdAttr = target?.getAttribute("data-plot");
    const sectionIdAttr = target?.getAttribute("data-section");
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    if (editing && plotIdAttr) {
      const p = byId.get(plotIdAttr)!;
      drag.current = { kind: "plot", id: p.id, sx: e.clientX, sy: e.clientY, vx: view.x, vy: view.y, orig: geo(p) };
    } else if (editing && sectionIdAttr) {
      const s = sections.data!.find((x) => x.id === sectionIdAttr)!;
      const og = movedSections[s.id] ?? { x: s.mapX, y: s.mapY, w: s.mapW, h: s.mapH, rot: s.mapRot };
      const children = items.filter((p) => p.sectionId === s.id).map((p) => [p.id, geo(p)] as [string, Geo]);
      drag.current = { kind: "section", id: s.id, sx: e.clientX, sy: e.clientY, vx: view.x, vy: view.y, orig: og, children };
    } else {
      drag.current = { kind: "pan", sx: e.clientX, sy: e.clientY, vx: view.x, vy: view.y };
    }
  };
  const onPointerMove = (e: RPointerEvent<SVGSVGElement>) => {
    const d = drag.current;
    if (!d) return;
    const dx = (e.clientX - d.sx) / view.scale;
    const dy = (e.clientY - d.sy) / view.scale;
    if (d.kind === "pan") setView((v) => ({ ...v, x: d.vx - dx, y: d.vy - dy }));
    else if (d.kind === "plot" && d.orig) setMoved((m) => ({ ...m, [d.id!]: { ...d.orig!, x: Math.round(d.orig!.x + dx), y: Math.round(d.orig!.y + dy) } }));
    else if (d.kind === "section" && d.orig) {
      const rx = Math.round(dx), ry = Math.round(dy);
      setMovedSections((m) => ({ ...m, [d.id!]: { ...d.orig!, x: d.orig!.x + rx, y: d.orig!.y + ry } }));
      setMoved((m) => {
        const n = { ...m };
        for (const [pid, g] of d.children!) n[pid] = { ...g, x: g.x + rx, y: g.y + ry };
        return n;
      });
    }
  };
  const onPointerUp = (e: RPointerEvent<SVGSVGElement>) => {
    const d = drag.current;
    drag.current = null;
    const still = Math.abs(e.clientX - (d?.sx ?? 0)) < 4 && Math.abs(e.clientY - (d?.sy ?? 0)) < 4;
    if (!still) return;
    const target = (e.target as Element).closest("[data-plot]");
    const pid = target?.getAttribute("data-plot");
    if (pid) {
      if (e.shiftKey && editing) setMulti((m) => (m.includes(pid) ? m.filter((x) => x !== pid) : [...m, pid].slice(-2)));
      else {
        setSelected(pid);
        setMulti([pid]);
      }
    } else if (d?.kind === "pan" && !(e.target as Element).closest("[data-section]")) {
      setSelected(null);
    }
    const sid = (e.target as Element).closest("[data-section]")?.getAttribute("data-section");
    if (sid && view.scale < DETAIL_SCALE && !editing) {
      const s = sections.data?.find((x) => x.id === sid);
      if (s) {
        const scale = Math.min(size.w / s.mapW, size.h / s.mapH) * 0.9;
        setView({ scale, x: s.mapX - (size.w / scale - s.mapW) / 2, y: s.mapY - (size.h / scale - s.mapH) / 2 });
      }
    }
  };

  // wheel zoom (non-passive listener)
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      zoomAt(e.deltaY < 0 ? 1.18 : 1 / 1.18, e.clientX - r.left, e.clientY - r.top);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [size]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = 60 / view.scale;
    if (e.key === "ArrowLeft") setView((v) => ({ ...v, x: v.x - step }));
    else if (e.key === "ArrowRight") setView((v) => ({ ...v, x: v.x + step }));
    else if (e.key === "ArrowUp") setView((v) => ({ ...v, y: v.y - step }));
    else if (e.key === "ArrowDown") setView((v) => ({ ...v, y: v.y + step }));
    else if (e.key === "+" || e.key === "=") zoomAt(1.25);
    else if (e.key === "-") zoomAt(0.8);
    else if (e.key === "0") fit();
    else return;
    e.preventDefault();
  };

  // ---- virtualisation: only plots intersecting the viewport are rendered
  const vp: Box = { x: view.x, y: view.y, w: size.w / view.scale, h: size.h / view.scale };
  const detail = view.scale >= DETAIL_SCALE;
  const visible = useMemo(() => {
    if (!detail) return [];
    const ids = queryGrid(grid, vp);
    const out: MapPlot[] = [];
    for (const id of ids) {
      const p = byId.get(id);
      if (p) out.push(p);
      if (out.length >= MAX_DRAWN) break;
    }
    // moved plots stay visible while dragged
    for (const id of Object.keys(moved)) if (!ids.has(id) && byId.get(id)) out.push(byId.get(id)!);
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [grid, byId, detail, vp.x, vp.y, vp.w, vp.h, moved]);

  const sectionStats = useMemo(() => {
    const m = new Map<string, Record<string, number>>();
    for (const p of items) {
      const r = m.get(p.sectionId) ?? {};
      r[p.status] = (r[p.status] ?? 0) + 1;
      m.set(p.sectionId, r);
    }
    return m;
  }, [items]);

  const rotateSelected = () => {
    for (const id of multi.length ? multi : selected ? [selected] : []) {
      const p = byId.get(id);
      if (!p) continue;
      const g = geo(p);
      setMoved((m) => ({ ...m, [id]: { ...g, rot: (g.rot + 15) % 360 } }));
    }
  };
  const saveGeometry = async () => {
    try {
      const plotItems = await Promise.all(Object.entries(moved).map(async ([ident, g]) => [await resolveId(current!.id, ident), g.x, g.y, g.w, g.h, g.rot]));
      if (plotItems.length) await api("plots.geometry", { entity: "plot", items: plotItems });
      const secItems = Object.entries(movedSections).map(([id, g]) => [id, g.x, g.y, g.w, g.h, g.rot]);
      if (secItems.length) await api("plots.geometry", { entity: "section", items: secItems });
      setMoved({});
      setMovedSections({});
      await invalidate();
      toast(t("common.savedToast"));
    } catch (e) {
      toast(errText(e), "error");
    }
  };

  const changes = Object.keys(moved).length + Object.keys(movedSections).length;
  const selectedPlot = selected ? byId.get(selected) : undefined;

  return (
    <>
      <PageHeader
        title={t("map.title")}
        subtitle={current?.name}
        actions={
          <>
            {current && <PrintButton doc="map" args={{ cemeteryId: current.id, paper: "A3" }} label={t("map.export")} />}
            {can("edit_map") && (
              <Button variant={editing ? "primary" : "secondary"} icon={<Pencil className="size-5" aria-hidden />} onClick={() => setEditing((e) => !e)} aria-pressed={editing} data-tour="map-edit">
                {t("map.edit")}
              </Button>
            )}
          </>
        }
      />
      {editing && (
        <div className="mb-3 flex flex-wrap items-center gap-3 rounded-lg border border-accent bg-lichen/60 px-4 py-3">
          <span className="mr-auto">{t("map.editOn")}</span>
          <Button icon={<RotateCw className="size-4" aria-hidden />} onClick={rotateSelected} disabled={!selected && !multi.length}>
            {t("map.rotate")}
          </Button>
          <Button icon={<Combine className="size-4" aria-hidden />} onClick={() => setMerging(true)} disabled={multi.length !== 2} title={t("map.mergeHint")}>
            {t("map.merge")}
          </Button>
          {changes > 0 && <span className="font-semibold">{t("map.changes", { count: changes })}</span>}
          <Button variant="primary" icon={<Save className="size-4" aria-hidden />} onClick={saveGeometry} disabled={!changes}>
            {t("map.save")}
          </Button>
          <Button variant="quiet" icon={<X className="size-4" aria-hidden />} onClick={() => (setMoved({}), setMovedSections({}))} disabled={!changes}>
            {t("map.discard")}
          </Button>
        </div>
      )}
      <div className="grid gap-4 xl:grid-cols-[1fr_20rem]">
        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <Button aria-label={t("map.zoomIn")} onClick={() => zoomAt(1.4)}>
              <Plus className="size-5" />
            </Button>
            <Button aria-label={t("map.zoomOut")} onClick={() => zoomAt(1 / 1.4)}>
              <Minus className="size-5" />
            </Button>
            <Button icon={<Maximize2 className="size-4" aria-hidden />} onClick={fit}>
              {t("map.fit")}
            </Button>
            <form
              className="ml-auto flex gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                const q = jump.trim().toUpperCase();
                const p = items.find((x) => x.identifier.toUpperCase() === q);
                if (p) focusPlot(p);
                else toast(t("map.notFound", { q }), "error");
              }}
            >
              <Input aria-label={t("map.jump")} placeholder={t("map.jumpHint")} value={jump} onChange={(e) => setJump(e.target.value)} className="w-40" />
              <Button type="submit" className="whitespace-nowrap">{t("map.jumpButton")}</Button>
            </form>
          </div>
          <div
            ref={wrapRef}
            tabIndex={0}
            onKeyDown={onKeyDown}
            aria-label={`${t("map.title")}. ${t("map.keyboardHelp")}`}
            className="relative h-[68vh] min-h-[420px] overflow-hidden rounded-xl border border-seam bg-[#e3eae5] dark:bg-[#0f1a16]"
          >
            {plots.isLoading ? (
              <Spinner />
            ) : (
              <svg
                width={size.w}
                height={size.h}
                className={cx("touch-none select-none", editing ? "cursor-move" : "cursor-grab")}
                onPointerDown={onPointerDown}
                onPointerMove={onPointerMove}
                onPointerUp={onPointerUp}
                role="img"
                aria-label={t("map.title")}
                data-testid="map-svg"
              >
                <defs>
                  <pattern id="hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                    <rect width="6" height="6" fill={statusColor.unusable} />
                    <line x1="0" y1="0" x2="0" y2="6" stroke="#ffffff55" strokeWidth="2" />
                  </pattern>
                  <pattern id="stripes" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                    <rect width="6" height="6" fill={statusColor.notified} />
                    <line x1="0" y1="0" x2="0" y2="6" stroke="#ffffff66" strokeWidth="2" />
                  </pattern>
                </defs>
                <g transform={`scale(${view.scale}) translate(${-view.x} ${-view.y})`}>
                  {(sections.data ?? []).map((s) => {
                    const g = movedSections[s.id] ?? { x: s.mapX, y: s.mapY, w: s.mapW, h: s.mapH, rot: 0 };
                    const stats = sectionStats.get(s.id) ?? {};
                    const total = Object.values(stats).reduce((a, b) => a + b, 0) || 1;
                    let acc = 0;
                    return (
                      <g key={s.id} data-section={s.id}>
                        <rect x={g.x} y={g.y} width={g.w} height={g.h} rx={2} fill="var(--c-slab)" stroke="var(--c-seam)" strokeWidth={1.5 / view.scale} />
                        {!detail &&
                          PLOT_STATUSES.map((st) => {
                            const n = stats[st] ?? 0;
                            if (!n) return null;
                            const w = (g.w * n) / total;
                            const r = <rect key={st} x={g.x + acc} y={g.y + g.h - Math.min(g.h * 0.25, 12 / view.scale)} width={w} height={Math.min(g.h * 0.25, 12 / view.scale)} fill={st === "notified" ? "url(#stripes)" : st === "unusable" ? "url(#hatch)" : statusColor[st]} />;
                            acc += w;
                            return r;
                          })}
                        <text x={g.x} y={g.y - 5 / view.scale} fontSize={16 / view.scale} fontFamily="Atkinson Hyperlegible Next, sans-serif" fontWeight={600} fill="var(--c-slate)" pointerEvents="none">
                          {s.code}
                        </text>
                      </g>
                    );
                  })}
                  {visible.map((p) => {
                    const g = geo(p);
                    const isSel = p.id === selected || multi.includes(p.id);
                    const fill = p.status === "notified" ? "url(#stripes)" : p.status === "unusable" ? "url(#hatch)" : statusColor[p.status];
                    return (
                      <g key={p.id} data-plot={p.id} transform={g.rot ? `rotate(${g.rot} ${g.x + g.w / 2} ${g.y + g.h / 2})` : undefined}>
                        <rect
                          x={g.x}
                          y={g.y}
                          width={g.w}
                          height={g.h}
                          rx={Math.min(g.w, g.h) * 0.12}
                          fill={fill}
                          stroke={isSel ? "var(--c-slate)" : p.protected ? "#b08d2f" : p.status === "free" ? "var(--c-granite)" : "none"}
                          strokeWidth={(isSel ? 3 : p.protected ? 2.5 : 1) / view.scale}
                        >
                          <title>
                            {p.identifier} – {t(`status.${p.status}`)}
                          </title>
                        </rect>
                        {view.scale >= LABEL_SCALE && (
                          <text x={g.x + g.w / 2} y={g.y + g.h / 2} textAnchor="middle" dominantBaseline="central" fontSize={Math.min(g.w * 0.45, 12 / view.scale + 2)} fill={p.status === "free" || p.status === "expired" ? "#1f2a30" : "#ffffff"} pointerEvents="none">
                            {p.identifier.split("/").slice(-1)[0]}
                          </text>
                        )}
                      </g>
                    );
                  })}
                </g>
              </svg>
            )}
            <p className="pointer-events-none absolute bottom-2 left-3 rounded bg-slab/90 px-2 text-sm text-granite">{detail ? t("map.visible", { count: visible.length }) : t("map.overview")}</p>
          </div>
        </div>
        <aside className="flex flex-col gap-4">
          {selectedPlot && current ? <SelectedPanel plot={selectedPlot} cemeteryId={current.id} onOpen={(id) => nav.go({ view: "plot", id })} /> : <Alert>{t("map.keyboardHelp")}</Alert>}
          <div className="rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)] px-4 py-3">
            <h2 className="mb-2 text-lg">{t("map.legend")}</h2>
            <ul className="flex flex-col gap-1.5">
              {PLOT_STATUSES.map((s) => (
                <li key={s} className="flex items-center justify-between gap-2">
                  <StatusBadge status={s} />
                  <span className="text-granite">{fmtNum(items.filter((p) => p.status === s).length)}</span>
                </li>
              ))}
              <li className="flex items-center gap-2">
                <StatusDot status="occupied" protectedPlot /> {t("status.protected")}
              </li>
            </ul>
          </div>
        </aside>
      </div>
      {merging && multi.length === 2 && current && <MergeDialog cemeteryId={current.id} names={[multi[0], multi[1]]} onClose={() => (setMerging(false), setMulti([]))} />}
    </>
  );
}

function SelectedPanel({ plot, cemeteryId, onOpen }: { plot: MapPlot; cemeteryId: string; onOpen: (id: string) => void }) {
  const { t } = useTranslation();
  const d = useQuery({
    queryKey: ["mapPlot", cemeteryId, plot.identifier],
    queryFn: async () => api<PlotDetail>("plots.get", { id: await resolveId(cemeteryId, plot.identifier) }),
  });
  return (
    <div className="rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)] px-4 py-4" data-testid="map-selected">
      <p className="mb-1 text-sm text-granite">{t("map.selected")}</p>
      <Plaque identifier={plot.identifier} large />
      <div className="mt-2">
        <StatusBadge status={plot.status as PlotStatus} protectedPlot={plot.protected} />
      </div>
      <p className="text-granite">{d.data?.plotType.name}</p>
      {d.data && (
        <ul className="mt-3 flex flex-col gap-1">
          {d.data.occupants.map((o) => (
            <li key={o.eventId}>
              <strong>{o.deceasedName}</strong>
              <span className="block text-sm text-granite">
                {fmtDate(o.birthDate)} – {fmtDate(o.deathDate)}
              </span>
            </li>
          ))}
          {d.data.currentLease && (
            <li className="mt-2 text-sm">
              {t("fields.leaseExpiry")}: {fmtDate(d.data.currentLease.expiryDate)}
            </li>
          )}
        </ul>
      )}
      <Button variant="primary" className="mt-4 w-full" disabled={!d.data} onClick={() => d.data && onOpen(d.data.plot.id)}>
        {t("wizard.openPlot")}
      </Button>
    </div>
  );
}

function MergeDialog({ cemeteryId, names, onClose }: { cemeteryId: string; names: string[]; onClose: () => void }) {
  const { t } = useTranslation();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("map.merge")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={reason.trim().length < 3}
            onClick={async () => {
              try {
                const [targetId, absorbedId] = await Promise.all(names.map((n) => resolveId(cemeteryId, n)));
                await api("plots.merge", { targetId, absorbedId, reason });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("map.merge")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <p className="flex items-center gap-2">
          <Plaque identifier={names[0]} /> + <Plaque identifier={names[1]} />
        </p>
        <p className="text-granite">{t("map.mergeHint")}</p>
        <ReasonField value={reason} onChange={setReason} />
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}
