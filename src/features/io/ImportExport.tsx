import { useTranslation } from "react-i18next";
import { PageHeader, Spinner } from "../../components/ui";

export function ImportExport() {
  const { t } = useTranslation();
  return (
    <>
      <PageHeader title={t("nav.importExport")} />
      <Spinner />
    </>
  );
}
