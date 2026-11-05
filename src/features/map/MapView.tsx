import { useTranslation } from "react-i18next";
import { PageHeader, Spinner } from "../../components/ui";

export function MapView(_props: { plotId?: string }) {
  const { t } = useTranslation();
  return (
    <>
      <PageHeader title={t("nav.map")} />
      <Spinner />
    </>
  );
}
