import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Copy, ExternalLink } from "lucide-react";
import { api } from "../../lib/api";
import { useInvalidate } from "../../lib/queries";
import { Alert, Button, Checkbox, ErrorAlert, Panel, Spinner, Textarea } from "../../components/ui";

interface Embed {
  token: string;
  enabled: boolean;
  fees: boolean;
  url: string;
  iframe: string;
}

/** FR-UGY-02: publishing the grave search on the operator's website. */
export function PublicSearchTab() {
  const { t } = useTranslation();
  const invalidate = useInvalidate();
  const q = useQuery({ queryKey: ["public-embed"], queryFn: () => api<Embed>("public.embed") });
  const [error, setError] = useState<unknown>(null);
  const [copied, setCopied] = useState(false);
  const [on, setOn] = useState<boolean | null>(null);
  const [fees, setFees] = useState<boolean | null>(null);
  const e = q.data;
  const enabled = on ?? e?.enabled ?? false;
  return (
    <Panel title={t("publicSearch.title")}>
      {!e ? (
        <Spinner />
      ) : (
        <div className="flex max-w-3xl flex-col gap-5">
          <p>{t("publicSearch.intro")}</p>
          <Checkbox
            label={t("publicSearch.enable")}
            hint={t("publicSearch.enableHint")}
            checked={enabled}
            onChange={async (v) => {
              setError(null);
              setOn(v);
              try {
                await api("settings.set", { values: { "public.search_enabled": v ? "true" : "false" } });
                await invalidate();
              } catch (err) {
                setOn(null);
                setError(err);
              }
            }}
          />
          <ErrorAlert error={error} />
          {enabled && (
            <>
              <Checkbox
                label={t("publicSearch.fees")}
                hint={t("publicSearch.feesHint")}
                checked={fees ?? e.fees}
                onChange={async (v) => {
                  setError(null);
                  setFees(v);
                  try {
                    await api("settings.set", { values: { "public.fees_enabled": v ? "true" : "false" } });
                  } catch (err) {
                    setFees(null);
                    setError(err);
                  }
                }}
              />
              <div className="flex flex-col gap-2">
                <p className="font-semibold">{t("publicSearch.embedCode")}</p>
                <Textarea readOnly rows={3} value={e.iframe} className="font-mono text-sm" name="embedCode" onFocus={(x) => x.currentTarget.select()} />
                <div className="flex flex-wrap gap-2">
                  <Button
                    icon={<Copy className="size-4" aria-hidden />}
                    onClick={async () => {
                      await navigator.clipboard?.writeText(e.iframe).catch(() => undefined);
                      setCopied(true);
                    }}
                  >
                    {copied ? t("publicSearch.copied") : t("publicSearch.copy")}
                  </Button>
                  <a className="inline-flex min-h-11 items-center gap-2 rounded-lg border border-seam px-4 font-semibold hover:bg-lichen/40" href={e.url} target="_blank" rel="noopener">
                    <ExternalLink className="size-4" aria-hidden />
                    {t("publicSearch.open")}
                  </a>
                </div>
              </div>
              <Alert>{t("publicSearch.privacy")}</Alert>
            </>
          )}
        </div>
      )}
    </Panel>
  );
}
