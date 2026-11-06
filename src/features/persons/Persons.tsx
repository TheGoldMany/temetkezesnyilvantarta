import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { UserPlus, Pencil, FileDown } from "lucide-react";
import { useApp } from "../../app/context";
import { api } from "../../lib/api";
import type { Holder, Person } from "../../lib/types";
import { Alert, Button, DefinitionList, Dialog, Empty, ErrorAlert, Field, Input, PageHeader, Panel, Plaque, Spinner, cx } from "../../components/ui";
import { PersonFields, emptyPerson } from "../../components/forms";
import { fmtDate, fullName } from "../../lib/format";
import { useInvalidate } from "../../lib/queries";

export function PersonsList() {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(false);
  const { data, isLoading } = useQuery({ queryKey: ["persons", q], queryFn: () => api<Person[]>("persons.list", { query: q, limit: 200 }) });
  return (
    <>
      <PageHeader
        title={t("persons.title")}
        subtitle={t("persons.subtitle")}
        actions={
          can("record_events") && (
            <Button variant="primary" icon={<UserPlus className="size-5" aria-hidden />} onClick={() => setCreating(true)}>
              {t("persons.new")}
            </Button>
          )
        }
      />
      <div className="mb-5 max-w-md">
        <Field label={t("common.search")}>
          <Input value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
        </Field>
      </div>
      {isLoading ? (
        <Spinner />
      ) : !data?.length ? (
        <Empty>{t("persons.empty")}</Empty>
      ) : (
        <ul className="divide-y divide-seam rounded-xl border border-seam bg-slab shadow-[0_1px_2px_rgba(20,59,49,0.05)]">
          {data.map((p) => (
            <li key={p.id}>
              <button type="button" onClick={() => nav.go({ view: "person", id: p.id })} className="flex w-full items-center justify-between gap-4 px-5 py-3 text-left hover:bg-limestone">
                <span className="font-semibold">{fullName(p)}</span>
                <span className="text-granite">{p.masked ? t("common.hidden") : p.address}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {creating && <PersonDialog person={{ ...emptyPerson, id: "" } as Person} onClose={() => setCreating(false)} />}
    </>
  );
}

export function PersonView({ id }: { id: string }) {
  const { t } = useTranslation();
  const { nav, can } = useApp();
  const p = useQuery({ queryKey: ["person", id], queryFn: () => api<Person>("persons.get", { id }) });
  const plots = useQuery({ queryKey: ["personPlots", id], queryFn: () => api<Holder[]>("persons.plots", { id }) });
  const [editing, setEditing] = useState(false);
  if (!p.data) return <Spinner />;
  const x = p.data;
  return (
    <>
      <PageHeader
        title={fullName(x)}
        actions={
          <>
            {can("view_personal") && (
              <Button
                icon={<FileDown className="size-5" aria-hidden />}
                title={t("persons.gdprHint")}
                onClick={async () => {
                  const data = await api<object>("persons.dataExport", { id });
                  const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }));
                  const a = document.createElement("a");
                  a.href = url;
                  a.download = `adatkiadas-${fullName(x).replace(/\s+/g, "-").toLowerCase()}.json`;
                  document.body.appendChild(a);
                  a.click();
                  a.remove();
                  setTimeout(() => URL.revokeObjectURL(url), 60_000);
                }}
              >
                {t("persons.gdpr")}
              </Button>
            )}
            {can("record_events") && (
              <Button icon={<Pencil className="size-5" aria-hidden />} onClick={() => setEditing(true)}>
                {t("common.edit")}
              </Button>
            )}
          </>
        }
      />
      <div className="grid gap-6 lg:grid-cols-2">
        <Panel title={t("wizard.steps.people")}>
          {x.masked && (
            <div className="mb-3">
              <Alert>{t("persons.masked")}</Alert>
            </div>
          )}
          <DefinitionList
            items={[
              [t("fields.address"), x.address || "–"],
              [t("fields.mailAddress"), x.mailAddress || "–"],
              [t("fields.phone"), x.phone || "–"],
              [t("fields.email"), x.email || "–"],
              [t("common.notes"), x.notes || "–"],
            ]}
          />
        </Panel>
        <Panel title={t("persons.plots")}>
          {!plots.data?.length ? (
            <p className="text-granite">{t("common.none")}</p>
          ) : (
            <ul className="divide-y divide-seam">
              {plots.data.map((h) => (
                <li key={h.id}>
                  <button type="button" onClick={() => nav.go({ view: "plot", id: h.plotId })} className={cx("flex w-full items-center justify-between gap-3 py-3 text-left hover:bg-limestone/60", h.endDate && "text-granite")}>
                    <span>
                      {t(`holder.role.${h.role}`)}, {h.endDate ? `${fmtDate(h.startDate)} – ${fmtDate(h.endDate)}` : t("plots.since", { date: fmtDate(h.startDate) })}
                    </span>
                    <Plaque identifier={h.plotIdentifier} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>
      {editing && <PersonDialog person={x} onClose={() => setEditing(false)} />}
    </>
  );
}

function PersonDialog({ person, onClose }: { person: Person; onClose: () => void }) {
  const { t } = useTranslation();
  const [v, setV] = useState<Partial<Person>>(person);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const invalidate = useInvalidate();
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={person.id ? t("common.edit") : t("persons.new")}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            busy={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await api("persons.save", { person: v });
                await invalidate();
                onClose();
              } catch (e) {
                setError(e);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {person.masked && <Alert>{t("persons.masked")}</Alert>}
        <PersonFields value={v} onChange={setV} />
        <ErrorAlert error={error} />
      </div>
    </Dialog>
  );
}
