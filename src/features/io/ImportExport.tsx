import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FileUp, Keyboard, ShieldCheck } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import { Alert, Button, Checkbox, ErrorAlert, Field, PageHeader, Panel, Select } from "../../components/ui";
import { useCurrentCemetery, useInvalidate, usePlotTypes } from "../../lib/queries";
import { ExportButton } from "./ExportButton";

interface ReadResult {
  headers: string[];
  rows: string[][];
  rowCount: number;
  sheetNames: string[];
  mapping: Record<string, number>;
  targets: string[];
}
interface RowError {
  row: number;
  field: string;
  code: string;
  message: string;
}
interface ImportReport {
  dryRun: boolean;
  committed: boolean;
  rowsTotal: number;
  rowsOk: number;
  rowsFailed: number;
  plotsCreated: number;
  warnings: RowError[];
  errors: RowError[];
  preview: Record<string, string>[];
}
type Source = { path: string } | { fileName: string; content: string };

/** FR-IMP-01 (wizard with dry run), FR-IMP-04 (full export), UC-05 check report. */
export function ImportExport() {
  const { t } = useTranslation();
  const { can, nav } = useApp();
  const { current } = useCurrentCemetery();
  const types = usePlotTypes();
  const invalidate = useInvalidate();
  const [source, setSource] = useState<Source | null>(null);
  const [sheet, setSheet] = useState<string>("");
  const [read, setRead] = useState<ReadResult | null>(null);
  const [mapping, setMapping] = useState<Record<string, number>>({});
  const [keepFolio, setKeepFolio] = useState(true);
  const [createLeases, setCreateLeases] = useState(true);
  const [skipInvalid, setSkipInvalid] = useState(false);
  const [defType, setDefType] = useState("coffin_burial");
  const [defPlotType, setDefPlotType] = useState("");
  const [report, setReport] = useState<ImportReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [check, setCheck] = useState<{ deceasedWithoutPlot: string[]; leasedPlotsWithoutDeceased: string[]; occupiedPlotsWithoutLease: string[] } | null>(null);

  const load = async (src: Source, sh?: string) => {
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      const r = await api<ReadResult>("import.read", { ...src, sheet: sh || null });
      setSource(src);
      setRead(r);
      setMapping(r.mapping);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const onFile = (f: File | undefined) => {
    if (!f) return;
    const r = new FileReader();
    r.onload = () => void load({ fileName: f.name, content: String(r.result).split("base64,")[1] ?? "" });
    r.readAsDataURL(f);
  };
  const run = async (dryRun: boolean) => {
    if (!source || !current) return;
    setBusy(true);
    setError(null);
    try {
      const rep = await api<ImportReport>("import.run", {
        ...source,
        sheet: sheet || null,
        options: { cemeteryId: current.id, mapping, keepFolio, createLeases, skipInvalid, dryRun, defaultEventType: defType, defaultPlotTypeId: defPlotType },
      });
      setReport(rep);
      if (rep.committed) await invalidate();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const columnFor = (target: string) => mapping[target] ?? -1;
  const setColumn = (target: string, col: number) =>
    setMapping((m) => {
      const n = { ...m };
      if (col < 0) delete n[target];
      else n[target] = col;
      return n;
    });

  return (
    <>
      <PageHeader
        title={t("io.title")}
        actions={
          can("record_events") &&
          can("import") && (
            <Button icon={<Keyboard className="size-5" aria-hidden />} onClick={() => nav.go({ view: "quickEntry" })}>
              {t("io.quickEntry")}
            </Button>
          )
        }
      />
      <div className="flex flex-col gap-6">
        {can("import") && (
          <Panel title={t("io.importTitle")}>
            <div className="flex flex-col gap-4">
              <p className="text-granite">{t("io.importIntro")}</p>
              <div className="flex flex-wrap items-center gap-3">
                <label className="inline-flex cursor-pointer items-center gap-2 rounded-md border border-seam bg-slab px-4 py-2 font-semibold">
                  <FileUp className="size-5" aria-hidden />
                  {t("io.pickFile")}
                  <input type="file" accept=".xlsx,.xls,.ods,.csv,.txt" className="sr-only" data-testid="import-file" onChange={(e) => onFile(e.target.files?.[0])} />
                </label>
                {read && <span className="text-granite">{t("io.rows", { count: read.rowCount })}</span>}
                {read && read.sheetNames.length > 1 && (
                  <Select
                    aria-label={t("io.sheet")}
                    value={sheet}
                    onChange={(e) => {
                      setSheet(e.target.value);
                      if (source) void load(source, e.target.value);
                    }}
                    className="max-w-60"
                  >
                    {read.sheetNames.map((n) => (
                      <option key={n} value={n}>
                        {n}
                      </option>
                    ))}
                  </Select>
                )}
              </div>
              {read && (
                <>
                  <h3 className="text-lg">{t("io.mapping")}</h3>
                  <p className="text-granite">{t("io.mappingHint")}</p>
                  <div className="max-h-[50vh] overflow-y-auto rounded-md border border-seam">
                    <table className="w-full">
                      <thead className="sticky top-0 bg-slab">
                        <tr className="text-left text-granite">
                          <th className="px-3 py-2">{t("io.target")}</th>
                          <th className="px-3 py-2">{t("io.column")}</th>
                          <th className="px-3 py-2">{t("io.example")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {read.targets.map((target) => {
                          const col = columnFor(target);
                          return (
                            <tr key={target} className="border-t border-seam">
                              <td className="px-3 py-1.5 font-semibold">{t(`importTarget.${target}`)}</td>
                              <td className="px-3 py-1.5">
                                <Select aria-label={t(`importTarget.${target}`)} value={col} onChange={(e) => setColumn(target, Number(e.target.value))} className="min-h-10">
                                  <option value={-1}>{t("io.notImported")}</option>
                                  {read.headers.map((h, i) => (
                                    <option key={i} value={i}>
                                      {h || `#${i + 1}`}
                                    </option>
                                  ))}
                                </Select>
                              </td>
                              <td className="px-3 py-1.5 text-granite">{col >= 0 ? read.rows[0]?.[col] : ""}</td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                  <h3 className="text-lg">{t("io.options")}</h3>
                  <div className="grid gap-4 md:grid-cols-2">
                    <Field label={t("io.defaultEventType")}>
                      <Select value={defType} onChange={(e) => setDefType(e.target.value)}>
                        {["coffin_burial", "urn_placement"].map((et) => (
                          <option key={et} value={et}>
                            {t(`eventType.${et}`)}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field label={t("io.defaultPlotType")}>
                      <Select value={defPlotType} onChange={(e) => setDefPlotType(e.target.value)}>
                        <option value="">{types.data?.find((x) => x.code === "single")?.name}</option>
                        {(types.data ?? []).map((pt) => (
                          <option key={pt.id} value={pt.id}>
                            {pt.name}
                          </option>
                        ))}
                      </Select>
                    </Field>
                  </div>
                  <Checkbox label={t("io.keepFolio")} checked={keepFolio} onChange={setKeepFolio} />
                  <Checkbox label={t("io.createLeases")} checked={createLeases} onChange={setCreateLeases} />
                  <Checkbox label={t("io.skipInvalid")} checked={skipInvalid} onChange={setSkipInvalid} />
                  <div className="flex flex-wrap gap-3">
                    <Button variant="primary" busy={busy} onClick={() => run(true)} icon={<ShieldCheck className="size-5" aria-hidden />}>
                      {t("io.dryRun")}
                    </Button>
                    <Button busy={busy} disabled={!report?.dryRun || report.rowsOk === 0} onClick={() => run(false)}>
                      {t("io.run")}
                    </Button>
                  </div>
                </>
              )}
              <ErrorAlert error={error} />
              {report && <ReportView report={report} />}
            </div>
          </Panel>
        )}
        {can("export") && (
          <Panel title={t("io.exportTitle")}>
            <p className="mb-3 text-granite">{t("io.exportIntro")}</p>
            <div className="flex flex-wrap gap-3">
              <ExportButton kind="full" format="xlsx" args={{}} label={`${t("io.exportFull")} (Excel)`} />
              <ExportButton kind="full" format="csv" args={{}} label={`${t("io.exportFull")} (CSV)`} />
              <ExportButton kind="full" format="json" args={{}} label={`${t("io.exportFull")} (JSON)`} />
              <ExportButton kind="register" args={{ cemeteryId: current?.id }} label={t("io.exportRegister")} />
              <ExportButton kind="plots" args={{ cemeteryId: current?.id }} label={t("io.exportPlots")} />
              <ExportButton kind="nori" args={{ cemeteryId: current?.id }} label={t("io.exportNori")} />
            </div>
          </Panel>
        )}
        <Panel title={t("io.checkTitle")} actions={<Button onClick={async () => setCheck(await api("import.check", { cemeteryId: current?.id ?? null }))}>{t("io.checkRun")}</Button>}>
          <p className="text-granite">{t("io.checkIntro")}</p>
          {check && (
            <div className="mt-4 grid gap-4 md:grid-cols-3" data-testid="migration-check">
              {(
                [
                  ["deceasedWithoutPlot", check.deceasedWithoutPlot],
                  ["leasedWithoutDeceased", check.leasedPlotsWithoutDeceased],
                  ["occupiedWithoutLease", check.occupiedPlotsWithoutLease],
                ] as [string, string[]][]
              ).map(([k, items]) => (
                <div key={k} className="rounded-md border border-seam p-3">
                  <p className="font-semibold">
                    {t(`io.${k}`)}: {items.length}
                  </p>
                  {items.length === 0 ? (
                    <p className="text-sm text-granite">{t("io.checkOk")}</p>
                  ) : (
                    <ul className="mt-1 max-h-48 overflow-y-auto text-sm">
                      {items.map((i) => (
                        <li key={i}>{i}</li>
                      ))}
                    </ul>
                  )}
                </div>
              ))}
            </div>
          )}
          {check && <div className="mt-3"><ExportButton kind="migration_check" args={{ cemeteryId: current?.id }} label="Excel" /></div>}
        </Panel>
      </div>
    </>
  );
}

function ReportView({ report }: { report: ImportReport }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-3" data-testid="import-report">
      {report.dryRun ? (
        <Alert tone={report.rowsFailed ? "warn" : "ok"}>{t("io.dryResult", { ok: report.rowsOk, failed: report.rowsFailed, plots: report.plotsCreated })}</Alert>
      ) : report.committed ? (
        <Alert tone="ok">{t("io.runResult", { ok: report.rowsOk, plots: report.plotsCreated })}</Alert>
      ) : (
        <Alert tone="error">{t("io.notCommitted")}</Alert>
      )}
      {report.errors.length > 0 && (
        <details open>
          <summary className="cursor-pointer font-semibold text-alert">
            {t("io.errors")} ({report.errors.length})
          </summary>
          <ul className="mt-2 max-h-60 overflow-y-auto text-sm">
            {report.errors.map((e, i) => (
              <li key={i}>
                {t("io.row", { row: e.row })}: {e.field && `${t(`importTarget.${e.field}`, { defaultValue: e.field })} – `}
                {e.message}
              </li>
            ))}
          </ul>
        </details>
      )}
      {report.warnings.length > 0 && (
        <details>
          <summary className="cursor-pointer font-semibold text-warn">
            {t("io.warnings")} ({report.warnings.length})
          </summary>
          <ul className="mt-2 max-h-60 overflow-y-auto text-sm">
            {report.warnings.map((e, i) => (
              <li key={i}>
                {t("io.row", { row: e.row })}: {e.message}
              </li>
            ))}
          </ul>
        </details>
      )}
      {report.preview.length > 0 && (
        <details>
          <summary className="cursor-pointer font-semibold">{t("io.preview")}</summary>
          <table className="mt-2 w-full text-sm">
            <tbody>
              {report.preview.map((p, i) => (
                <tr key={i} className="border-t border-seam">
                  <td className="py-1 pr-2">{p.folio}</td>
                  <td className="py-1 pr-2">{p.eventDate}</td>
                  <td className="py-1 pr-2 font-semibold">{p.name}</td>
                  <td className="py-1 pr-2">{p.plot}</td>
                  <td className="py-1 pr-2">{t(`eventType.${p.eventType}`)}</td>
                  <td className="py-1">{p.holder}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
    </div>
  );
}
