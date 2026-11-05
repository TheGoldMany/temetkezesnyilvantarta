import { useTranslation } from "react-i18next";
import { PageHeader, Spinner } from "../../components/ui";

export function Prints() {
  const { t } = useTranslation();
  return (
    <>
      <PageHeader title={t("nav.prints")} />
      <Spinner />
    </>
  );
}
