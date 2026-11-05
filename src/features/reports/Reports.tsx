import { useTranslation } from "react-i18next";
import { PageHeader, Spinner } from "../../components/ui";

export function Reports() {
  const { t } = useTranslation();
  return (
    <>
      <PageHeader title={t("nav.reports")} />
      <Spinner />
    </>
  );
}
