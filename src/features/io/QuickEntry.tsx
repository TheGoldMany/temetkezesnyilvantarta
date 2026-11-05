import { useTranslation } from "react-i18next";
import { PageHeader, Spinner } from "../../components/ui";

export function QuickEntry() {
  const { t } = useTranslation();
  return (
    <>
      <PageHeader title={t("nav.importExport")} />
      <Spinner />
    </>
  );
}
