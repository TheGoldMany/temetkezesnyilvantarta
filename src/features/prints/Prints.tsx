import { useState } from "react";
import { useTranslation } from "react-i18next";
import { BookText, Landmark, ListOrdered, FileSearch, Map as MapIcon } from "lucide-react";
import { api } from "../../lib/api";
import { Alert, Button, ErrorAlert, Field, PageHeader, Panel, Select, TextField } from "../../components/ui";
import { useCurrentCemetery } from "../../lib/queries";
import { openGenerated, type GeneratedFile } from "./PrintButton";
import { useApp } from "../../app/context";

/** FR-NYO-01, FR-KER-03, FR-TER-05, JOG-16 */
export function Prints() {
  const { t } = useTranslation();
  const { can } = useApp();
  const { current } = useCurrentCemetery();
  const year = new Date().getFullYear();
  const [from, setFrom] = useState(`${year}-01-01`);
  const [to, setTo] = useState(`${year}-12-31`);
  const [paper, setPaper] = useState("A3");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [last, setLast] = useState<GeneratedFile | null>(null);
  const run = async (doc: string, args: Record<string, unknown>) => {
    setBusy(doc);
    setError(null);
    try {
      const f = await api<GeneratedFile>("print.document", { doc, args: { cemeteryId: current?.id, ...args } });
      setLast(f);
      await openGenerated(f);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(null);
    }
  };
  if (!can("print")) return <Alert tone="warn">{t("errors.forbidden")}</Alert>;
  const card = (key: string, Icon: typeof BookText, title: string, hint: string, action: () => void, extra?: React.ReactNode) => (
    <Panel key={key}>
      <div className="flex h-full flex-col gap-3">
        <h2 className="flex items-center gap-2 text-lg">
          <Icon className="size-5 text-moss" aria-hidden /> {title}
        </h2>
        <p className="text-granite">{hint}</p>
        {extra}
        <div className="mt-auto">
          <Button variant="primary" busy={busy === key} onClick={action}>
            {t("prints.generate")}
          </Button>
        </div>
      </div>
    </Panel>
  );
  return (
    <>
      <PageHeader title={t("prints.title")} subtitle={current?.name} />
      <div className="mb-6 flex flex-wrap items-end gap-4 rounded-lg border border-seam bg-slab px-5 py-4">
        <span className="font-semibold">{t("prints.period")}</span>
        <TextField label={t("common.from")} type="date" value={from} onChange={setFrom} />
        <TextField label={t("common.to")} type="date" value={to} onChange={setTo} />
      </div>
      <ErrorAlert error={error} />
      {last && (
        <div className="mb-4">
          <Alert tone="ok" title={t("prints.generated", { path: last.fileName })}>
            {t("prints.pages", { count: last.pages })}
            {last.volumeId && ` · ${t("prints.volume", { id: last.volumeId })} · ${t("prints.digest", { hash: last.contentHash?.slice(0, 16) + "…" })}`}
            <Button variant="quiet" className="ml-2" onClick={() => openGenerated(last)}>
              {t("prints.openFile")}
            </Button>
          </Alert>
        </div>
      )}
      <div className="grid gap-5 md:grid-cols-2">
        {card("register", BookText, t("prints.register"), t("prints.registerHint"), () => run("register", { dateFrom: from || null, dateTo: to || null }))}
        {card("crypt_book", Landmark, t("prints.cryptBook"), t("prints.cryptBookHint"), () => run("crypt_book", { dateFrom: from || null, dateTo: to || null }))}
        {card("index", ListOrdered, t("prints.index"), t("prints.indexHint"), () => run("index", {}))}
        {card("inspection", FileSearch, t("prints.inspection"), t("prints.inspectionHint"), () => run("inspection", {}))}
        {card(
          "map",
          MapIcon,
          t("prints.map"),
          t("prints.mapHint"),
          () => run("map", { paper }),
          <Field label={t("prints.paper")}>
            <Select value={paper} onChange={(e) => setPaper(e.target.value)} className="max-w-40">
              {["A4", "A3", "A2", "A1", "A0"].map((p) => (
                <option key={p}>{p}</option>
              ))}
            </Select>
          </Field>,
        )}
        <Panel>
          <h2 className="mb-2 text-lg">{t("prints.forms")}</h2>
          <p className="text-granite">{t("prints.formsHint")}</p>
          <p className="mt-3 text-sm text-granite">{t("prints.lastPrints")}</p>
        </Panel>
      </div>
    </>
  );
}
