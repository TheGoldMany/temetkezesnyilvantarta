import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Building2 } from "lucide-react";
import { useApp } from "../../app/context";
import { shell } from "../../lib/api";
import { Button, ErrorAlert, Panel, TextField } from "../../components/ui";

/** Moves the sign-in to another organisation of the same person. A fresh page
 *  load drops everything cached from the previous organisation. */
export async function switchOrganization(accountId: string) {
  await shell.switchOrg(accountId);
  window.location.assign(window.location.pathname);
}

/** Top-bar switcher, shown when the person belongs to several organisations. */
export function OrgSwitcher() {
  const { t } = useTranslation();
  const { status } = useApp();
  const orgs = status?.account?.organizations ?? [];
  const [busy, setBusy] = useState(false);
  if (orgs.length < 2) return null;
  return (
    <label className="flex items-center gap-1.5 text-sm text-granite">
      <Building2 className="size-4 shrink-0" aria-hidden />
      <span className="sr-only">{t("orgs.switch")}</span>
      <select
        aria-label={t("orgs.switch")}
        className="max-w-48 rounded-md border border-seam bg-slab px-2 py-1 text-sm text-granite"
        value={orgs.find((o) => o.current)?.accountId ?? ""}
        disabled={busy}
        onChange={async (e) => {
          setBusy(true);
          try {
            await switchOrganization(e.target.value);
          } finally {
            setBusy(false);
          }
        }}
      >
        {orgs.map((o) => (
          <option key={o.accountId} value={o.accountId}>
            {o.name}
          </option>
        ))}
      </select>
    </label>
  );
}

/** Fiókom › Szervezeteim: where the person works, and a new organisation with the same sign-in. */
export function OrganizationsPanel() {
  const { t } = useTranslation();
  const { status } = useApp();
  const orgs = status?.account?.organizations ?? [];
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  if (!status?.account) return null;
  return (
    <Panel title={t("orgs.title")}>
      <div className="flex max-w-2xl flex-col gap-4">
        <p>{t("orgs.intro")}</p>
        <ul className="flex flex-col divide-y divide-seam border-y border-seam" data-testid="org-list">
          {orgs.map((o) => (
            <li key={o.accountId} className="flex items-center justify-between gap-3 py-2">
              <span className="font-semibold">{o.name}</span>
              {o.current ? (
                <span className="text-sm text-granite">{t("orgs.current")}</span>
              ) : (
                <Button variant="quiet" onClick={() => switchOrganization(o.accountId)}>
                  {t("orgs.open")}
                </Button>
              )}
            </li>
          ))}
        </ul>
        <form
          className="flex flex-wrap items-end gap-3"
          onSubmit={async (e) => {
            e.preventDefault();
            setError(null);
            setBusy(true);
            try {
              await shell.createOrg(name);
              window.location.assign(window.location.pathname);
            } catch (err) {
              setError(err);
              setBusy(false);
            }
          }}
        >
          <TextField className="w-72" label={t("orgs.newName")} hint={t("orgs.newHint")} value={name} onChange={setName} />
          <Button type="submit" variant="secondary" busy={busy} disabled={name.trim().length < 2}>
            {t("orgs.create")}
          </Button>
        </form>
        <ErrorAlert error={error} />
      </div>
    </Panel>
  );
}
