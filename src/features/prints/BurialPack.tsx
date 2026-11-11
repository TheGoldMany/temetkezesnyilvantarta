import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { Checkbox, ErrorAlert, Field, Select } from "../../components/ui";
import { PrintButton } from "./PrintButton";

interface PackItem {
  key: string;
  title: string;
  default: boolean;
}

const SYMBOL_KEY = "temisz.mourningSymbol";

/** Every printout a burial needs (mourning card, notice, dig sheet, grave
 *  marker, certificate, fee request…) pre-selected, in one PDF. */
export function BurialPack({ eventId }: { eventId: string }) {
  const { t } = useTranslation();
  const q = useQuery({
    queryKey: ["burialItems", eventId],
    queryFn: () =>
      api<{ name: string; items: PackItem[] }>("print.burialItems", {
        eventId,
      }),
  });
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [symbol, setSymbol] = useState(() => localStorage.getItem(SYMBOL_KEY) ?? "none");
  useEffect(() => {
    if (q.data) setPicked(new Set(q.data.items.filter((i) => i.default).map((i) => i.key)));
  }, [q.data]);

  if (q.error) return <ErrorAlert error={q.error} />;
  if (!q.data || q.data.items.length === 0) return null;
  const items = q.data.items.filter((i) => picked.has(i.key)).map((i) => i.key);
  const hasCard = picked.has("mourning_card");
  return (
    <section className="flex flex-col gap-3 border-t border-seam pt-4" data-testid="burial-pack">
      <div>
        <h2 className="text-lg font-semibold">{t("burialPack.title")}</h2>
        <p className="text-sm text-slate">{t("burialPack.hint")}</p>
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        {q.data.items.map((i) => (
          <Checkbox
            key={i.key}
            label={i.title}
            checked={picked.has(i.key)}
            onChange={(v) =>
              setPicked((s) => {
                const n = new Set(s);
                if (v) n.add(i.key);
                else n.delete(i.key);
                return n;
              })
            }
          />
        ))}
      </div>
      {hasCard && (
        <Field label={t("burialPack.symbol")} className="max-w-xs">
          <Select
            value={symbol}
            onChange={(e) => {
              setSymbol(e.target.value);
              localStorage.setItem(SYMBOL_KEY, e.target.value);
            }}
          >
            <option value="none">{t("burialPack.symbolNone")}</option>
            <option value="cross">{t("burialPack.symbolCross")}</option>
          </Select>
        </Field>
      )}
      <div>
        {items.length > 0 ? (
          <PrintButton variant="primary" doc="burial_pack" args={{ eventId, items, symbol }} label={t("burialPack.print", { count: items.length })} />
        ) : (
          <p className="text-sm text-granite">{t("burialPack.none")}</p>
        )}
      </div>
    </section>
  );
}
