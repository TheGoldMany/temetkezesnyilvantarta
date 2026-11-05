import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { useApp } from "../../app/context";
import { api, command, shell } from "../../lib/api";
import { PERMISSIONS, type Cemetery, type FeeRate, type PlotType, type User } from "../../lib/types";
import { Alert, Button, Checkbox, Dialog, ErrorAlert, Field, Input, PageHeader, Panel, Select, Spinner, Tabs, TextField, useErrorText } from "../../components/ui";
import { fmtDate, fmtDateTime, fmtHuf, todayIso } from "../../lib/format";
import { useCemeteries, useCurrentCemetery, useInvalidate, usePlotTypes, useSections } from "../../lib/queries";
import { useToast } from "../../app/toast";
import { BackupTab } from "./BackupTab";
import { SecurityPanel } from "../account/Security";
import { SubscriptionTab, huf } from "../account/Subscription";
import { AuditTab } from "./AuditTab";

type Tab = "operator" | "cemeteries" | "plotTypes" | "fees" | "rules" | "users" | "account" | "subscription" | "backup" | "audit" | "appearance" | "about";

export function SettingsView({ tab }: { tab?: string }) {
  const { t } = useTranslation();
  const { can } = useApp();
  const [current, setCurrent] = useState<Tab>((tab as Tab) ?? "operator");
  const tabs: Tab[] = ["operator", "cemeteries", "plotTypes", "fees", "rules", ...(can("manage_users") ? (["users"] as Tab[]) : []), "account", ...(can("manage_settings") ? (["subscription"] as Tab[]) : []), "backup", "audit", "appearance", "about"];
  return (
    <>
      <PageHeader title={t("settings.title")} />
      <Tabs value={current} onChange={setCurrent} items={tabs.map((x) => ({ value: x, label: t(`settings.tabs.${x}`) }))} />
      <div className="mt-6">
        {current === "operator" && <SettingsForm keys={OPERATOR_KEYS} />}
        {current === "cemeteries" && <CemeteriesTab />}
        {current === "plotTypes" && <PlotTypesTab />}
        {current === "fees" && <FeesTab />}
        {current === "rules" && <SettingsForm keys={RULE_KEYS} />}
        {current === "users" && <UsersTab />}
        {current === "subscription" && <SubscriptionTab />}
        {current === "account" && (
          <div className="flex flex-col gap-6">
            <SecurityPanel />
            <OwnPassword />
          </div>
        )}
        {current === "backup" && <BackupTab />}
        {current === "audit" && <AuditTab />}
        {current === "appearance" && <AppearanceTab />}
        {current === "about" && <AboutTab />}
      </div>
    </>
  );
}

type KeyDef = { key: string; label: string; type?: "text" | "number" | "bool" | "logo"; hint?: string };
const OPERATOR_KEYS: KeyDef[] = [
  { key: "operator.name", label: "settings.operatorName" },
  { key: "operator.address", label: "settings.operatorAddress" },
  { key: "operator.phone", label: "settings.operatorPhone" },
  { key: "operator.email", label: "settings.operatorEmail" },
  { key: "operator.tax_number", label: "settings.taxNumber" },
  { key: "signer.name", label: "settings.signerName" },
  { key: "signer.title", label: "settings.signerTitle" },
  { key: "regulation.name", label: "settings.regulation" },
  { key: "operator.logo", label: "settings.logo", type: "logo" },
];
const RULE_KEYS: KeyDef[] = [
  { key: "rules.grace_period_days", label: "settings.gracePeriod", type: "number" },
  { key: "rules.urn_rest_period_years", label: "settings.urnRestPeriod", type: "number" },
  { key: "rules.expiry_warning_days", label: "settings.expiryWarning", type: "number" },
  { key: "security.login_required", label: "settings.loginRequired", type: "bool", hint: "settings.loginRequiredOffWarning" },
  { key: "security.idle_logout_minutes", label: "settings.idleLogout", type: "number" },
];

function SettingsForm({ keys }: { keys: KeyDef[] }) {
  const { t } = useTranslation();
  const { refresh, can, status } = useApp();
  const toast = useToast();
  const { data } = useQuery({ queryKey: ["settings"], queryFn: () => api<Record<string, string>>("settings.all") });
  const [v, setV] = useState<Record<string, string>>({});
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    if (data) setV(data);
  }, [data]);
  if (!data) return <Spinner />;
  const save = async () => {
    setError(null);
    try {
      const values: Record<string, string> = {};
      for (const k of keys) if (v[k.key] !== undefined && v[k.key] !== data[k.key]) values[k.key] = v[k.key];
      // single-user mode logs in as the current user (Q10)
      if (values["security.login_required"] === "false" && status?.session?.userId) values["security.auto_login_user"] = status.session.userId;
      await api("settings.set", { values });
      await refresh();
      toast(t("common.savedToast"));
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Panel>
      <div className="flex max-w-2xl flex-col gap-4">
        {keys.map((k) =>
          k.type === "bool" ? (
            <div key={k.key} className="flex flex-col gap-2">
              <Checkbox label={t(k.label)} checked={v[k.key] !== "false"} onChange={(b) => setV({ ...v, [k.key]: b ? "true" : "false" })} />
              {k.hint && v[k.key] === "false" && <Alert tone="warn">{t(k.hint)}</Alert>}
            </div>
          ) : k.type === "logo" ? (
            <Field key={k.key} label={t(k.label)}>
              <div className="flex items-center gap-4">
                {v[k.key] && <img src={v[k.key]} alt="" className="h-16 max-w-40 rounded border border-seam bg-white object-contain p-1" />}
                <input
                  type="file"
                  accept="image/png,image/jpeg"
                  onChange={(e) => {
                    const f = e.target.files?.[0];
                    if (!f) return;
                    const r = new FileReader();
                    r.onload = () => setV({ ...v, [k.key]: String(r.result) });
                    r.readAsDataURL(f);
                  }}
                />
              </div>
            </Field>
          ) : (
            <TextField key={k.key} label={t(k.label)} type={k.type === "number" ? "number" : "text"} value={v[k.key] ?? ""} onChange={(x) => setV({ ...v, [k.key]: x })} />
          ),
        )}
        <ErrorAlert error={error} />
        {can("manage_settings") && (
          <div>
            <Button variant="primary" onClick={save}>
              {t("common.saveChanges")}
            </Button>
          </div>
        )}
      </div>
    </Panel>
  );
}

const emptyCemetery: Cemetery = { id: "", name: "", address: "", parcelNumber: "", kind: "public", owner: "", operator: "", openingHours: "", folioScope: "continuous", notes: "" };

function CemeteriesTab() {
  const { t } = useTranslation();
  const { can } = useApp();
  const cems = useCemeteries();
  const { current, setCemeteryId } = useCurrentCemetery();
  const sections = useSections(current?.id);
  const [editing, setEditing] = useState<Cemetery | null>(null);
  const [bulk, setBulk] = useState(false);
  return (
    <div className="flex flex-col gap-6">
      <Panel
        title={t("common.cemetery")}
        actions={
          can("manage_structure") && (
            <Button icon={<Plus className="size-4" aria-hidden />} onClick={() => setEditing({ ...emptyCemetery })}>
              {t("settings.newCemetery")}
            </Button>
          )
        }
      >
        <ul className="divide-y divide-seam">
          {(cems.data ?? []).map((c) => (
            <li key={c.id} className="flex flex-wrap items-center justify-between gap-3 py-3">
              <span>
                <span className="font-semibold">{c.name}</span>
                <span className="block text-sm text-granite">
                  {c.address}, {c.kind === "public" ? t("settings.kindPublic") : t("settings.kindDenominational")}, {c.folioScope === "yearly" ? t("settings.folioYearly") : t("settings.folioContinuous")}
                </span>
              </span>
              <span className="flex gap-2">
                {current?.id !== c.id && <Button onClick={() => setCemeteryId(c.id)}>{t("common.open")}</Button>}
                {can("manage_structure") && <Button onClick={() => setEditing(c)}>{t("common.edit")}</Button>}
              </span>
            </li>
          ))}
        </ul>
      </Panel>
      {current && (
        <Panel
          title={`${t("settings.sections")} – ${current.name}`}
          actions={
            can("manage_structure") && (
              <Button variant="primary" icon={<Plus className="size-4" aria-hidden />} onClick={() => setBulk(true)}>
                {t("settings.bulkCreate")}
              </Button>
            )
          }
        >
          <table className="w-full">
            <thead>
              <tr className="text-left text-granite">
                <th className="py-2">{t("settings.sectionCode")}</th>
                <th className="py-2">{t("settings.sectionName")}</th>
                <th className="py-2">{t("plots.row")}</th>
                <th className="py-2">{t("plots.number")}</th>
              </tr>
            </thead>
            <tbody>
              {(sections.data ?? []).map((s) => (
                <tr key={s.id} className="border-t border-seam">
                  <td className="py-2 font-semibold">{s.code}</td>
                  <td className="py-2">{s.name}</td>
                  <td className="py-2">{s.rowCount}</td>
                  <td className="py-2">{s.plotCount}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Panel>
      )}
      {editing && <CemeteryDialog cemetery={editing} onClose={() => setEditing(null)} />}
      {bulk && current && <BulkDialog cemeteryId={current.id} onClose={() => setBulk(false)} />}
    </div>
  );
}

/** FR-ELO-01: what a new cemetery adds to the subscription, shown before creating it. */
function CemeteryPriceNotice() {
  const { t } = useTranslation();
  const q = useQuery({
    queryKey: ["billing-quote"],
    queryFn: () => command<{ inTrial: boolean; monthlyNet?: number; yearlyNet?: number; plan?: string; unitNet?: number; nowGross?: number; periodEnd?: string }>("billing_quote_cemetery"),
  });
  if (!q.data) return null;
  const d = q.data;
  return (
    <Alert tone="info" title={t("billing.newCemeteryTitle")}>
      {d.plan
        ? t("billing.newCemeteryPaid", { now: huf(d.nowGross ?? 0), date: d.periodEnd, unit: huf(d.unitNet ?? 0), per: t(`billing.per.${d.plan}`) })
        : t("billing.newCemeteryTrial", { monthly: huf(d.monthlyNet ?? 45000), yearly: huf(d.yearlyNet ?? 450000) })}
    </Alert>
  );
}

function CemeteryDialog({ cemetery, onClose }: { cemetery: Cemetery; onClose: () => void }) {
  const { t } = useTranslation();
  const [c, setC] = useState(cemetery);
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  const { setCemeteryId } = useCurrentCemetery();
  const set = (k: keyof Cemetery) => (v: string) => setC({ ...c, [k]: v });
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={c.id ? c.name : t("settings.newCemetery")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              try {
                const saved = await api<Cemetery>("cemeteries.save", { cemetery: c });
                await invalidate();
                if (!cemetery.id) setCemeteryId(saved.id);
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        {!cemetery.id && (
          <div className="md:col-span-2">
            <CemeteryPriceNotice />
          </div>
        )}
        <TextField className="md:col-span-2" label={t("settings.cemeteryName")} value={c.name} onChange={set("name")} required />
        <TextField label={t("settings.operatorAddress")} value={c.address} onChange={set("address")} />
        <TextField label={t("settings.parcelNumber")} value={c.parcelNumber} onChange={set("parcelNumber")} />
        <Field label={t("settings.kind")}>
          <Select value={c.kind} onChange={(e) => set("kind")(e.target.value)}>
            <option value="public">{t("settings.kindPublic")}</option>
            <option value="denominational">{t("settings.kindDenominational")}</option>
          </Select>
        </Field>
        <Field label={t("settings.folioScope")}>
          <Select value={c.folioScope} onChange={(e) => set("folioScope")(e.target.value)}>
            <option value="continuous">{t("settings.folioContinuous")}</option>
            <option value="yearly">{t("settings.folioYearly")}</option>
          </Select>
        </Field>
        <TextField label={t("settings.owner")} value={c.owner} onChange={set("owner")} />
        <TextField label={t("settings.operator")} value={c.operator} onChange={set("operator")} />
        <TextField className="md:col-span-2" label={t("settings.openingHours")} value={c.openingHours} onChange={set("openingHours")} />
        <div className="md:col-span-2">
          <ErrorAlert error={error} />
        </div>
      </div>
    </Dialog>
  );
}

function BulkDialog({ cemeteryId, onClose }: { cemeteryId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const types = usePlotTypes();
  const [v, setV] = useState({ sectionCode: "", sectionName: "", rowFrom: "1", rowTo: "10", plotsPerRow: "20", firstPlotNumber: "1", plotTypeId: "" });
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const invalidate = useInvalidate();
  const toast = useToast();
  useEffect(() => {
    if (!v.plotTypeId && types.data?.[0]) setV((s) => ({ ...s, plotTypeId: types.data![0].id }));
  }, [types.data, v.plotTypeId]);
  const set = (k: keyof typeof v) => (x: string) => setV({ ...v, [k]: x });
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={t("settings.bulkCreate")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            busy={busy}
            onClick={async () => {
              setBusy(true);
              try {
                const r = await api<{ rowsCreated: number; plotsCreated: number }>("structure.bulkCreate", {
                  input: { cemeteryId, sectionCode: v.sectionCode, sectionName: v.sectionName, rowFrom: Number(v.rowFrom), rowTo: Number(v.rowTo), plotsPerRow: Number(v.plotsPerRow), firstPlotNumber: Number(v.firstPlotNumber), plotTypeId: v.plotTypeId },
                });
                await invalidate();
                toast(t("settings.bulkDone", { rows: r.rowsCreated, plots: r.plotsCreated }));
                onClose();
              } catch (e) {
                setError(e);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("settings.bulkCreate")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <p className="text-granite">{t("settings.bulkHint")}</p>
        <div className="grid gap-4 md:grid-cols-3">
          <TextField label={t("settings.sectionCode")} value={v.sectionCode} onChange={(x) => set("sectionCode")(x.toUpperCase())} required />
          <TextField className="md:col-span-2" label={t("settings.sectionName")} value={v.sectionName} onChange={set("sectionName")} />
          <TextField label={t("settings.rowFrom")} type="number" value={v.rowFrom} onChange={set("rowFrom")} />
          <TextField label={t("settings.rowTo")} type="number" value={v.rowTo} onChange={set("rowTo")} />
          <TextField label={t("settings.plotsPerRow")} type="number" value={v.plotsPerRow} onChange={set("plotsPerRow")} />
          <TextField label={t("settings.firstPlot")} type="number" value={v.firstPlotNumber} onChange={set("firstPlotNumber")} />
          <Field label={t("fields.plotType")} className="md:col-span-2">
            <Select value={v.plotTypeId} onChange={(e) => set("plotTypeId")(e.target.value)}>
              {(types.data ?? []).map((pt) => (
                <option key={pt.id} value={pt.id}>
                  {pt.name}
                </option>
              ))}
            </Select>
          </Field>
        </div>
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

function PlotTypesTab() {
  const { t } = useTranslation();
  const { can } = useApp();
  const types = usePlotTypes();
  const [editing, setEditing] = useState<PlotType | null>(null);
  return (
    <Panel
      actions={
        can("manage_settings") && (
          <Button icon={<Plus className="size-4" aria-hidden />} onClick={() => setEditing({ id: "", code: "", name: "", category: "grave", defaultLeaseYears: 25, restPeriodYears: 25, coffinCapacity: 1, urnCapacity: 4, widthCm: 100, lengthCm: 210, builtin: false, sortOrder: 200 })}>
            {t("settings.newType")}
          </Button>
        )
      }
    >
      <table className="w-full">
        <thead>
          <tr className="text-left text-granite">
            <th className="py-2">{t("settings.typeName")}</th>
            <th className="py-2">{t("settings.category")}</th>
            <th className="py-2">{t("settings.leaseYears")}</th>
            <th className="py-2">{t("settings.restYears")}</th>
            <th className="py-2">{t("settings.coffins")}</th>
            <th className="py-2">{t("settings.urns")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {(types.data ?? []).map((pt) => (
            <tr key={pt.id} className="border-t border-seam">
              <td className="py-2 font-semibold">{pt.name}</td>
              <td className="py-2">{t(`category.${pt.category}`)}</td>
              <td className="py-2">{pt.defaultLeaseYears}</td>
              <td className="py-2">{pt.restPeriodYears}</td>
              <td className="py-2">{pt.coffinCapacity}</td>
              <td className="py-2">{pt.urnCapacity}</td>
              <td className="py-2 text-right">{can("manage_settings") && <Button variant="quiet" onClick={() => setEditing(pt)}>{t("common.edit")}</Button>}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {editing && <PlotTypeDialog value={editing} onClose={() => setEditing(null)} />}
    </Panel>
  );
}

function PlotTypeDialog({ value, onClose }: { value: PlotType; onClose: () => void }) {
  const { t } = useTranslation();
  const [v, setV] = useState(value);
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  const n = (k: keyof PlotType) => (x: string) => setV({ ...v, [k]: x === "" ? null : Number(x) });
  return (
    <Dialog
      open
      onClose={onClose}
      title={v.id ? v.name : t("settings.newType")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              try {
                await api("plotTypes.save", { plotType: v });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        <TextField className="md:col-span-2" label={t("settings.typeName")} value={v.name} onChange={(x) => setV({ ...v, name: x })} required />
        <Field label={t("settings.category")} className="md:col-span-2">
          <Select value={v.category} onChange={(e) => setV({ ...v, category: e.target.value })}>
            {["grave", "crypt", "urn", "scatter", "common"].map((c) => (
              <option key={c} value={c}>
                {t(`category.${c}`)}
              </option>
            ))}
          </Select>
        </Field>
        <TextField label={t("settings.leaseYears")} type="number" value={v.defaultLeaseYears} onChange={n("defaultLeaseYears")} />
        <TextField label={t("settings.restYears")} type="number" value={v.restPeriodYears} onChange={n("restPeriodYears")} />
        <TextField label={t("settings.coffins")} type="number" value={v.coffinCapacity} onChange={n("coffinCapacity")} />
        <TextField label={t("settings.urns")} type="number" value={v.urnCapacity} onChange={n("urnCapacity")} />
        <TextField label="Szélesség (cm)" type="number" value={v.widthCm ?? ""} onChange={n("widthCm")} />
        <TextField label="Hossz (cm)" type="number" value={v.lengthCm ?? ""} onChange={n("lengthCm")} />
        <div className="md:col-span-2">
          <ErrorAlert error={error} />
        </div>
      </div>
    </Dialog>
  );
}

function FeesTab() {
  const { t } = useTranslation();
  const { can } = useApp();
  const { current } = useCurrentCemetery();
  const types = usePlotTypes();
  const fees = useQuery({ queryKey: ["fees", current?.id], queryFn: () => api<FeeRate[]>("feeRates.list", { cemeteryId: current?.id }), enabled: !!current });
  const [v, setV] = useState({ feeKind: "purchase", plotTypeId: "", amountHuf: "", validFrom: todayIso() });
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  const typeName = (id: string | null) => types.data?.find((x) => x.id === id)?.name ?? t("settings.anyType");
  return (
    <div className="flex flex-col gap-6">
      <Alert>{t("settings.feeHint")}</Alert>
      {can("manage_settings") && current && (
        <Panel title={t("settings.addFee")}>
          <div className="grid items-end gap-4 md:grid-cols-5">
            <Field label={t("settings.feeKind")}>
              <Select value={v.feeKind} onChange={(e) => setV({ ...v, feeKind: e.target.value })}>
                {["purchase", "renewal", "opening", "chapel", "maintenance"].map((k) => (
                  <option key={k} value={k}>
                    {t(`settings.feeKinds.${k}`)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={t("fields.plotType")}>
              <Select value={v.plotTypeId} onChange={(e) => setV({ ...v, plotTypeId: e.target.value })}>
                <option value="">{t("settings.anyType")}</option>
                {(types.data ?? []).map((pt) => (
                  <option key={pt.id} value={pt.id}>
                    {pt.name}
                  </option>
                ))}
              </Select>
            </Field>
            <TextField label={t("settings.amount")} type="number" value={v.amountHuf} onChange={(x) => setV({ ...v, amountHuf: x })} />
            <TextField label={t("settings.validFrom")} type="date" value={v.validFrom} onChange={(x) => setV({ ...v, validFrom: x })} />
            <Button
              variant="primary"
              disabled={!v.amountHuf}
              onClick={async () => {
                try {
                  await api("feeRates.add", { rate: { cemeteryId: current.id, plotTypeId: v.plotTypeId || null, feeKind: v.feeKind, amountHuf: Number(v.amountHuf), validFrom: v.validFrom, validTo: null } });
                  setV({ ...v, amountHuf: "" });
                  await invalidate();
                } catch (e) {
                  setError(e);
                }
              }}
            >
              {t("common.add")}
            </Button>
          </div>
          <div className="mt-3">
            <ErrorAlert error={error} />
          </div>
        </Panel>
      )}
      <Panel>
        <table className="w-full">
          <thead>
            <tr className="text-left text-granite">
              <th className="py-2">{t("settings.feeKind")}</th>
              <th className="py-2">{t("fields.plotType")}</th>
              <th className="py-2">{t("settings.amount")}</th>
              <th className="py-2">{t("settings.validFrom")}</th>
              <th className="py-2">{t("settings.validTo")}</th>
            </tr>
          </thead>
          <tbody>
            {(fees.data ?? []).map((f) => (
              <tr key={f.id} className="border-t border-seam">
                <td className="py-2">{t(`settings.feeKinds.${f.feeKind}`, { defaultValue: f.feeKind })}</td>
                <td className="py-2">{typeName(f.plotTypeId)}</td>
                <td className="py-2 font-semibold">{fmtHuf(f.amountHuf)}</td>
                <td className="py-2">{fmtDate(f.validFrom)}</td>
                <td className="py-2">{fmtDate(f.validTo)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Panel>
    </div>
  );
}

function UsersTab() {
  const { t } = useTranslation();
  const users = useQuery({ queryKey: ["users"], queryFn: () => api<User[]>("users.list") });
  const [editing, setEditing] = useState<User | null>(null);
  const [creating, setCreating] = useState(false);
  const invalidate = useInvalidate();
  return (
    <div className="flex flex-col gap-6">
      <Panel
        actions={
          <Button icon={<Plus className="size-4" aria-hidden />} onClick={() => setCreating(true)}>
            {t("settings.newUser")}
          </Button>
        }
      >
        <ul className="divide-y divide-seam">
          {(users.data ?? []).map((u) => (
            <li key={u.id} className="flex flex-wrap items-center justify-between gap-3 py-3">
              <span>
                <span className="font-semibold">{u.displayName}</span> <span className="text-granite">({u.username})</span>
                <span className="block text-sm text-granite">
                  {t(`role.${u.role}`)}, {u.active ? t("settings.active") : "inaktív"}, {fmtDateTime(u.lastLoginAt)}
                  {u.lockedUntil && `, ${t("settings.locked")}`}
                </span>
              </span>
              <span className="flex gap-2">
                {u.lockedUntil && (
                  <Button
                    onClick={async () => {
                      await api("users.unlock", { id: u.id });
                      await invalidate();
                    }}
                  >
                    {t("settings.unlock")}
                  </Button>
                )}
                <Button onClick={() => setEditing(u)}>{t("common.edit")}</Button>
              </span>
            </li>
          ))}
        </ul>
      </Panel>
      {(editing || creating) && <UserDialog user={editing} onClose={() => (setEditing(null), setCreating(false))} />}
    </div>
  );
}

function UserDialog({ user, onClose }: { user: User | null; onClose: () => void }) {
  const { t } = useTranslation();
  const [v, setV] = useState({ username: user?.username ?? "", displayName: user?.displayName ?? "", role: user?.role ?? "clerk", extra: user?.extraPermissions ?? [], active: user?.active ?? true });
  const [linkSent, setLinkSent] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const invalidate = useInvalidate();
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={user ? user.displayName : t("settings.newUser")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={async () => {
              try {
                if (user) {
                  await api("users.update", { id: user.id, displayName: v.displayName, role: v.role, extraPermissions: v.extra, active: v.active });
                } else {
                  await api("users.create", { username: v.username, displayName: v.displayName, role: v.role, extraPermissions: v.extra });
                }
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              }
            }}
          >
            {user ? t("common.save") : t("settings.sendInvite")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {!user && <p className="text-granite">{t("settings.inviteIntro")}</p>}
        <div className="grid gap-4 md:grid-cols-2">
          <TextField label={t("fields.displayName")} value={v.displayName} onChange={(x) => setV({ ...v, displayName: x })} required />
          <TextField label={t("fields.email")} type="email" value={v.username} onChange={(x) => setV({ ...v, username: x })} required inputProps={{ disabled: !!user, name: "email" }} />
          <Field label={t("fields.role")}>
            <Select value={v.role} onChange={(e) => setV({ ...v, role: e.target.value as User["role"] })}>
              {["admin", "clerk", "finance", "reader"].map((r) => (
                <option key={r} value={r}>
                  {t(`role.${r}`)}
                </option>
              ))}
            </Select>
          </Field>
        </div>
        {user && (
          <div className="flex flex-wrap items-center gap-3">
            <Button
              onClick={async () => {
                try {
                  await shell.userSendLink(user.id);
                  setLinkSent(true);
                } catch (e) {
                  setError(e);
                }
              }}
            >
              {t("settings.sendLink")}
            </Button>
            {linkSent && <span className="text-granite">{t("settings.linkSent")}</span>}
          </div>
        )}
        {user && <Checkbox label={t("settings.active")} checked={v.active} onChange={(b) => setV({ ...v, active: b })} />}
        <fieldset>
          <legend className="mb-2 font-semibold">{t("settings.extraPermissions")}</legend>
          <div className="grid gap-2 md:grid-cols-2">
            {PERMISSIONS.map((p) => (
              <Checkbox key={p} label={t(`permission.${p}`)} checked={v.extra.includes(p)} onChange={(b) => setV({ ...v, extra: b ? [...v.extra, p] : v.extra.filter((x) => x !== p) })} />
            ))}
          </div>
        </fieldset>
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}

function OwnPassword() {
  const { t } = useTranslation();
  const { status } = useApp();
  const [o, setO] = useState("");
  const [n, setN] = useState("");
  const toast = useToast();
  const errText = useErrorText();
  return (
    <Panel title={t("settings.changeOwnPassword")}>
      <div className="grid max-w-2xl items-end gap-4 md:grid-cols-3">
        <TextField label={t("settings.oldPassword")} type="password" value={o} onChange={setO} />
        <TextField label={t("settings.newPassword")} type="password" value={n} onChange={setN} />
        <Button
          onClick={async () => {
            try {
              await api("users.changePassword", { id: status?.session?.userId, oldPassword: o, newPassword: n });
              setO("");
              setN("");
              toast(t("common.savedToast"));
            } catch (e) {
              toast(errText(e), "error");
            }
          }}
        >
          {t("common.save")}
        </Button>
      </div>
    </Panel>
  );
}

function AppearanceTab() {
  const { t } = useTranslation();
  const { status, refresh } = useApp();
  const theme = status?.settings["ui.theme"] ?? "light";
  const zoom = Number(status?.settings["ui.zoom"] ?? 100);
  const set = async (values: Record<string, string>) => {
    await api("settings.set", { values }).catch(() => {
      for (const [k, v] of Object.entries(values)) localStorage.setItem(k === "ui.theme" ? "lunda.theme" : "lunda.zoom", v);
    });
    await refresh();
  };
  return (
    <Panel>
      <div className="flex max-w-xl flex-col gap-5">
        <Field label={t("settings.theme")}>
          <div className="flex gap-2">
            {["light", "dark"].map((x) => (
              <Button key={x} variant={theme === x ? "primary" : "secondary"} onClick={() => set({ "ui.theme": x })}>
                {x === "light" ? t("settings.themeLight") : t("settings.themeDark")}
              </Button>
            ))}
          </div>
        </Field>
        <Field label={`${t("settings.zoom")}: ${zoom} %`}>
          <Input type="range" min={90} max={150} step={10} value={zoom} onChange={(e) => set({ "ui.zoom": e.target.value })} />
        </Field>
      </div>
    </Panel>
  );
}

function AboutTab() {
  const { t } = useTranslation();
  const { status } = useApp();
  return (
    <Panel>
      <div className="flex max-w-2xl flex-col gap-3">
        <p className="wordmark text-2xl text-moss">{t("app.name")}</p>
        <p>{t("app.tagline")}</p>
        <p className="text-granite">{t("settings.version", { version: status?.version })}</p>
        <p className="text-granite">{t("settings.webAbout")}</p>
      </div>
    </Panel>
  );
}
