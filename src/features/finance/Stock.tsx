import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { ClipboardList, PackagePlus, Plus, ShoppingCart } from "lucide-react";
import { api } from "../../lib/api";
import { useInvalidate } from "../../lib/queries";
import { fmtHuf } from "../../lib/format";
import { Alert, Button, Checkbox, Dialog, Empty, ErrorAlert, Field, Input, Panel, Select, Spinner, TextField, cx } from "../../components/ui";

interface StockItem {
  id: string;
  cemeteryId: string;
  sku: string;
  name: string;
  unit: string;
  unitNetHuf: number;
  unitGrossHuf: number;
  vatRate: string;
  minQty: number;
  forSale: boolean;
  active: boolean;
  onHand: number;
  low: boolean;
}

interface Move {
  id: string;
  kind: "in" | "sale" | "use" | "adjust";
  qty: number;
  unitCostHuf: number | null;
  partner: string;
  note: string;
  movedOn: string;
  by: string | null;
}

const td = "py-2.5 pr-4 align-top";

/** Goods and materials: stock on hand, goods in, sales, use, stocktake. */
export function StockTab({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const [showAll, setShowAll] = useState(false);
  const q = useQuery({ queryKey: ["stock", cemeteryId, showAll], queryFn: () => api<StockItem[]>("stock.list", { cemeteryId, includeInactive: showAll }) });
  const [editing, setEditing] = useState<Partial<StockItem> | null>(null);
  const [receiving, setReceiving] = useState<StockItem | null>(null);
  const [using, setUsing] = useState<StockItem | null>(null);
  const [history, setHistory] = useState<StockItem | null>(null);
  const [selling, setSelling] = useState(false);
  const [counting, setCounting] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const items = q.data ?? [];
  const low = items.filter((i) => i.low);
  const after = async (msg: string) => {
    setDone(msg);
    await invalidate();
  };
  return (
    <div className="flex flex-col gap-6">
      {low.length > 0 && <Alert tone="warn">{t("stock.lowAlert", { names: low.map((i) => i.name).join(", ") })}</Alert>}
      {done && <Alert tone="ok">{done}</Alert>}
      <Panel
        title={t("stock.title")}
        actions={
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" icon={<ShoppingCart className="size-4" aria-hidden />} disabled={!items.some((i) => i.forSale && i.onHand > 0)} onClick={() => setSelling(true)}>
              {t("stock.sell")}
            </Button>
            <Button icon={<ClipboardList className="size-4" aria-hidden />} disabled={items.length === 0} onClick={() => setCounting(true)}>
              {t("stock.stocktake")}
            </Button>
            <Button icon={<Plus className="size-4" aria-hidden />} onClick={() => setEditing({ unit: "db", vatRate: "27", forSale: true, active: true, minQty: 0, unitNetHuf: 0 })}>
              {t("stock.newItem")}
            </Button>
          </div>
        }
      >
        <p className="mb-4 max-w-3xl text-granite">{t("stock.intro")}</p>
        {q.isLoading ? (
          <Spinner />
        ) : items.length === 0 ? (
          <Empty>{t("stock.empty")}</Empty>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full" data-testid="stock-table">
              <thead>
                <tr className="border-b border-seam text-left text-sm text-granite">
                  <th className="py-2 pr-4 font-medium">{t("stock.item")}</th>
                  <th className="py-2 pr-4 text-right font-medium">{t("stock.onHand")}</th>
                  <th className="py-2 pr-4 text-right font-medium">{t("stock.price")}</th>
                  <th className="py-2 pr-4 font-medium">{t("stock.minQty")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {items.map((i) => (
                  <tr key={i.id} className={cx("border-b border-seam last:border-0", !i.active && "text-granite")}>
                    <td className={td}>
                      <button type="button" className="text-left font-semibold underline-offset-3 hover:underline" onClick={() => setEditing(i)}>
                        {i.name}
                      </button>
                      <span className="block text-sm text-granite">
                        {[i.sku, i.forSale ? t("stock.goods") : t("stock.material"), !i.active && t("stock.inactive")].filter(Boolean).join(" · ")}
                      </span>
                    </td>
                    <td className={cx(td, "text-right font-semibold tabular-nums", i.low && "text-alert")}>
                      {i.onHand} {i.unit}
                      {i.low && <span className="block text-sm font-normal">{t("stock.low")}</span>}
                    </td>
                    <td className={cx(td, "text-right tabular-nums")}>{i.forSale ? fmtHuf(i.unitGrossHuf) : "–"}</td>
                    <td className={cx(td, "tabular-nums")}>{i.minQty > 0 ? `${i.minQty} ${i.unit}` : "–"}</td>
                    <td className={cx(td, "pr-0 text-right")}>
                      <div className="flex flex-wrap justify-end gap-2">
                        <Button icon={<PackagePlus className="size-4" aria-hidden />} onClick={() => setReceiving(i)}>
                          {t("stock.receive")}
                        </Button>
                        <Button variant="quiet" disabled={i.onHand <= 0} onClick={() => setUsing(i)}>
                          {t("stock.use")}
                        </Button>
                        <Button variant="quiet" onClick={() => setHistory(i)}>
                          {t("stock.moves")}
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <div className="mt-4">
          <Checkbox label={t("stock.showInactive")} checked={showAll} onChange={setShowAll} />
        </div>
      </Panel>
      <StocktakeHistory cemeteryId={cemeteryId} />

      {editing && <ItemDialog cemeteryId={cemeteryId} item={editing} onClose={() => setEditing(null)} onSaved={() => (setEditing(null), after(t("stock.saved")))} />}
      {receiving && <QtyDialog mode="receive" item={receiving} onClose={() => setReceiving(null)} onDone={(m) => (setReceiving(null), after(m))} />}
      {using && <QtyDialog mode="use" item={using} onClose={() => setUsing(null)} onDone={(m) => (setUsing(null), after(m))} />}
      {history && <MovesDialog item={history} onClose={() => setHistory(null)} />}
      {selling && <SaleDialog cemeteryId={cemeteryId} items={items.filter((i) => i.forSale && i.active && i.onHand > 0)} onClose={() => setSelling(false)} onDone={(m) => (setSelling(false), after(m))} />}
      {counting && <StocktakeDialog cemeteryId={cemeteryId} items={items.filter((i) => i.active)} onClose={() => setCounting(false)} onDone={(m) => (setCounting(false), after(m))} />}
    </div>
  );
}

function ItemDialog({ cemeteryId, item, onClose, onSaved }: { cemeteryId: string; item: Partial<StockItem>; onClose: () => void; onSaved: () => void }) {
  const { t } = useTranslation();
  const [f, setF] = useState(item);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const set = (p: Partial<StockItem>) => setF({ ...f, ...p });
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api("stock.save", { item: { ...f, cemeteryId, id: f.id ?? "", unitNetHuf: Number(f.unitNetHuf) || 0, minQty: Number(f.minQty) || 0 } });
      onSaved();
    } catch (e) {
      setError(e);
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={f.id ? t("stock.editItem") : t("stock.newItem")}
      footer={
        <>
          <Button variant="quiet" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" busy={busy} onClick={save}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        <TextField className="md:col-span-2" label={t("stock.name")} required value={f.name} onChange={(v) => set({ name: v })} inputProps={{ name: "itemName" }} />
        <TextField label={t("stock.sku")} hint={t("stock.skuHint")} value={f.sku} onChange={(v) => set({ sku: v })} />
        <TextField label={t("stock.unit")} hint={t("stock.unitHint")} value={f.unit} onChange={(v) => set({ unit: v })} />
        <TextField label={t("stock.netPrice")} type="number" value={f.unitNetHuf} onChange={(v) => set({ unitNetHuf: Number(v) })} inputProps={{ name: "unitNetHuf", min: 0 }} />
        <Field label={t("stock.vat")}>
          <Select value={f.vatRate} onChange={(e) => set({ vatRate: e.target.value })}>
            {["27", "18", "5", "0", "AAM", "TAM"].map((v) => (
              <option key={v} value={v}>
                {/^\d+$/.test(v) ? `${v}%` : v}
              </option>
            ))}
          </Select>
        </Field>
        <TextField label={t("stock.minQty")} hint={t("stock.minQtyHint")} type="number" value={f.minQty} onChange={(v) => set({ minQty: Number(v) })} inputProps={{ min: 0 }} />
        <div className="flex flex-col gap-3 md:col-span-2">
          <Checkbox label={t("stock.forSale")} hint={t("stock.forSaleHint")} checked={!!f.forSale} onChange={(v) => set({ forSale: v })} />
          {f.id && <Checkbox label={t("stock.active")} hint={t("stock.activeHint")} checked={!!f.active} onChange={(v) => set({ active: v })} />}
        </div>
      </div>
      <ErrorAlert error={error} />
    </Dialog>
  );
}

function QtyDialog({ mode, item, onClose, onDone }: { mode: "receive" | "use"; item: StockItem; onClose: () => void; onDone: (msg: string) => void }) {
  const { t } = useTranslation();
  const [qty, setQty] = useState("");
  const [cost, setCost] = useState("");
  const [partner, setPartner] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      const n = Number(qty);
      if (mode === "receive") await api("stock.receive", { itemId: item.id, qty: n, unitCostHuf: cost ? Number(cost) : null, partner, note });
      else await api("stock.use", { itemId: item.id, qty: n, note });
      onDone(t(mode === "receive" ? "stock.received" : "stock.usedDone", { qty: n, unit: item.unit, name: item.name }));
    } catch (e) {
      setError(e);
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={`${mode === "receive" ? t("stock.receive") : t("stock.use")}: ${item.name}`}
      footer={
        <>
          <Button variant="quiet" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" busy={busy} disabled={!(Number(qty) > 0)} onClick={go}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        <TextField label={`${t("stock.qty")} (${item.unit})`} type="number" required value={qty} onChange={setQty} inputProps={{ name: "qty", min: 1 }} />
        {mode === "receive" && <TextField label={t("stock.unitCost")} hint={t("stock.unitCostHint")} type="number" value={cost} onChange={setCost} />}
        {mode === "receive" && <TextField label={t("stock.supplier")} value={partner} onChange={setPartner} />}
        <TextField className={mode === "use" ? "md:col-span-2" : ""} label={mode === "use" ? t("stock.useFor") : t("stock.note")} value={note} onChange={setNote} />
      </div>
      <p className="mt-3 text-sm text-granite">{t("stock.onHandNow", { qty: item.onHand, unit: item.unit })}</p>
      <ErrorAlert error={error} />
    </Dialog>
  );
}

function SaleDialog({ cemeteryId, items, onClose, onDone }: { cemeteryId: string; items: StockItem[]; onClose: () => void; onDone: (msg: string) => void }) {
  const { t } = useTranslation();
  const [qty, setQty] = useState<Record<string, string>>({});
  const [payer, setPayer] = useState("");
  const [method, setMethod] = useState<"cash" | "invoice">("cash");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const lines = items.map((i) => ({ item: i, qty: Number(qty[i.id] || 0) })).filter((l) => l.qty > 0);
  const total = lines.reduce((s, l) => s + l.item.unitGrossHuf * l.qty, 0);
  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api<{ receipt?: { docNumber: string } }>("stock.sell", {
        sale: { cemeteryId, payerName: payer, method, lines: lines.map((l) => ({ itemId: l.item.id, qty: l.qty })) },
      });
      onDone(method === "cash" ? t("stock.soldCash", { total: fmtHuf(total), receipt: r.receipt?.docNumber ?? "" }) : t("stock.soldInvoice", { total: fmtHuf(total) }));
    } catch (e) {
      setError(e);
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={t("stock.sell")}
      footer={
        <>
          <Button variant="quiet" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" busy={busy} disabled={lines.length === 0 || !payer.trim()} onClick={go}>
            {method === "cash" ? t("stock.sellCash", { total: fmtHuf(total) }) : t("stock.sellInvoice")}
          </Button>
        </>
      }
    >
      <table className="w-full" data-testid="sale-table">
        <tbody>
          {items.map((i) => (
            <tr key={i.id} className="border-b border-seam last:border-0">
              <td className="py-2 pr-3">
                <span className="font-semibold">{i.name}</span>
                <span className="block text-sm text-granite">
                  {fmtHuf(i.unitGrossHuf)} / {i.unit} · {t("stock.inStock", { qty: i.onHand, unit: i.unit })}
                </span>
              </td>
              <td className="w-32 py-2">
                <Input type="number" min={0} max={i.onHand} aria-label={`${i.name}: ${t("stock.qty")}`} value={qty[i.id] ?? ""} onChange={(e) => setQty({ ...qty, [i.id]: e.target.value })} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="mt-4 grid gap-4 md:grid-cols-2">
        <TextField label={t("stock.buyer")} required value={payer} onChange={setPayer} inputProps={{ name: "payerName" }} />
        <Field label={t("stock.how")}>
          <Select value={method} onChange={(e) => setMethod(e.target.value as "cash" | "invoice")}>
            <option value="cash">{t("stock.howCash")}</option>
            <option value="invoice">{t("stock.howInvoice")}</option>
          </Select>
        </Field>
      </div>
      <p className="mt-4 text-right text-xl font-semibold tabular-nums">
        {t("stock.total")}: {fmtHuf(total)}
      </p>
      <ErrorAlert error={error} />
    </Dialog>
  );
}

function StocktakeDialog({ cemeteryId, items, onClose, onDone }: { cemeteryId: string; items: StockItem[]; onClose: () => void; onDone: (msg: string) => void }) {
  const { t } = useTranslation();
  const [counted, setCounted] = useState<Record<string, string>>({});
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const filled = items.filter((i) => counted[i.id] !== undefined && counted[i.id] !== "");
  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api<{ differences: number }>("stock.stocktake", { cemeteryId, note, counts: filled.map((i) => ({ itemId: i.id, counted: Number(counted[i.id]) })) });
      onDone(t("stock.stocktakeDone", { count: filled.length, diffs: r.differences }));
    } catch (e) {
      setError(e);
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={t("stock.stocktake")}
      footer={
        <>
          <Button variant="quiet" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" busy={busy} disabled={filled.length === 0} onClick={go}>
            {t("stock.stocktakeSave", { count: filled.length })}
          </Button>
        </>
      }
    >
      <p className="mb-3 text-granite">{t("stock.stocktakeIntro")}</p>
      <table className="w-full" data-testid="stocktake-table">
        <thead>
          <tr className="border-b border-seam text-left text-sm text-granite">
            <th className="py-2 pr-3 font-medium">{t("stock.item")}</th>
            <th className="py-2 pr-3 text-right font-medium">{t("stock.expected")}</th>
            <th className="py-2 font-medium">{t("stock.counted")}</th>
          </tr>
        </thead>
        <tbody>
          {items.map((i) => {
            const v = counted[i.id];
            const diff = v !== undefined && v !== "" ? Number(v) - i.onHand : 0;
            return (
              <tr key={i.id} className="border-b border-seam last:border-0">
                <td className="py-2 pr-3 font-semibold">{i.name}</td>
                <td className="py-2 pr-3 text-right tabular-nums">
                  {i.onHand} {i.unit}
                </td>
                <td className="w-40 py-2">
                  <Input type="number" min={0} aria-label={`${i.name}: ${t("stock.counted")}`} value={v ?? ""} onChange={(e) => setCounted({ ...counted, [i.id]: e.target.value })} />
                  {diff !== 0 && <span className={cx("text-sm font-semibold", diff < 0 ? "text-alert" : "text-moss")}>{diff > 0 ? `+${diff}` : diff}</span>}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <TextField className="mt-4" label={t("stock.note")} value={note} onChange={setNote} />
      <ErrorAlert error={error} />
    </Dialog>
  );
}

function MovesDialog({ item, onClose }: { item: StockItem; onClose: () => void }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["stock-moves", item.id], queryFn: () => api<Move[]>("stock.moves", { itemId: item.id }) });
  return (
    <Dialog open wide onClose={onClose} title={`${t("stock.moves")}: ${item.name}`}>
      {!q.data ? (
        <Spinner />
      ) : q.data.length === 0 ? (
        <Empty>{t("stock.noMoves")}</Empty>
      ) : (
        <table className="w-full text-sm">
          <tbody>
            {q.data.map((m) => (
              <tr key={m.id} className="border-b border-seam last:border-0">
                <td className="py-2 pr-3 whitespace-nowrap tabular-nums">{m.movedOn}</td>
                <td className="py-2 pr-3">{t(`stock.kind.${m.kind}`)}</td>
                <td className={cx("py-2 pr-3 text-right font-semibold tabular-nums", m.qty < 0 ? "text-alert" : "text-moss")}>
                  {m.qty > 0 ? `+${m.qty}` : m.qty} {item.unit}
                </td>
                <td className="py-2 pr-3">{[m.partner, m.note].filter(Boolean).join(" – ")}</td>
                <td className="py-2 text-granite">{m.by}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Dialog>
  );
}

function StocktakeHistory({ cemeteryId }: { cemeteryId: string }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["stocktakes", cemeteryId], queryFn: () => api<{ id: string; takenOn: string; note: string; by: string | null; lines: { name: string; unit: string; expected: number; counted: number; diff: number }[] }[]>("stock.stocktakes", { cemeteryId }) });
  if (!q.data || q.data.length === 0) return null;
  return (
    <Panel title={t("stock.stocktakes")}>
      <ul className="flex flex-col gap-3">
        {q.data.map((s) => {
          const diffs = s.lines.filter((l) => l.diff !== 0);
          return (
            <li key={s.id} className="border-b border-seam pb-3 last:border-0">
              <p className="font-semibold">
                {s.takenOn} · {t("stock.stocktakeSummary", { count: s.lines.length, diffs: diffs.length })}
                {s.by && <span className="font-normal text-granite"> · {s.by}</span>}
              </p>
              {s.note && <p className="text-granite">{s.note}</p>}
              {diffs.length > 0 && <p className="text-sm text-granite">{diffs.map((l) => `${l.name}: ${l.diff > 0 ? "+" : ""}${l.diff} ${l.unit}`).join(", ")}</p>}
            </li>
          );
        })}
      </ul>
    </Panel>
  );
}
