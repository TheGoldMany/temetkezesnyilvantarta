import { useTranslation } from "react-i18next";
import { useApp } from "../../app/context";
import { Tabs } from "../../components/ui";

/** Nyomtatás és riportok: one menu item, two pages. */
export function ToolsTabs({ value }: { value: "prints" | "reports" }) {
  const { t } = useTranslation();
  const { nav } = useApp();
  return (
    <div className="mb-6">
      <Tabs
        value={value}
        onChange={(v) => nav.go({ view: v })}
        items={[
          { value: "prints", label: t("nav.prints") },
          { value: "reports", label: t("nav.reports") },
        ]}
      />
    </div>
  );
}
