import { useApp } from "./app/context";
import { Shell } from "./app/Shell";
import { Spinner } from "./components/ui";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Login } from "./features/auth/Login";
import { TotpRequired } from "./features/account/Security";
import { PlatformAdmin } from "./features/account/PlatformAdmin";
import { FinanceView } from "./features/finance/Finance";
import { OperationsView } from "./features/operations/Operations";
import { Help } from "./features/help/Help";
import { shell } from "./lib/api";
import { useToast } from "./app/toast";
import { Onboarding } from "./features/auth/Onboarding";
import { Dashboard } from "./features/dashboard/Dashboard";
import { BurialsList } from "./features/burials/BurialsList";
import { NewBurial } from "./features/burials/NewBurial";
import { DeceasedView } from "./features/burials/DeceasedView";
import { PlotsList } from "./features/plots/PlotsList";
import { PlotDetailView } from "./features/plots/PlotDetail";
import { PersonsList, PersonView } from "./features/persons/Persons";
import { Expiries } from "./features/expiries/Expiries";
import { SettingsView } from "./features/settings/Settings";
import { MapView } from "./features/map/MapView";
import { Prints } from "./features/prints/Prints";
import { Reports } from "./features/reports/Reports";
import { ImportExport } from "./features/io/ImportExport";
import { QuickEntry } from "./features/io/QuickEntry";

/** #megerosites=… links from the verification e-mail work signed in or out. */
function useEmailVerificationLink() {
  const { setStatus } = useApp();
  const toast = useToast();
  const { t } = useTranslation();
  useEffect(() => {
    const check = () => {
      const h = window.location.hash;
      if (!h.startsWith("#megerosites=")) return;
      const token = h.slice("#megerosites=".length);
      history.replaceState(null, "", window.location.pathname + window.location.search);
      shell.verifyEmail(token).then(
        (s) => {
          setStatus(s);
          toast(t("account.verified"));
        },
        () => toast(t("errors.token_invalid"), "error"),
      );
    };
    check();
    window.addEventListener("hashchange", check);
    return () => window.removeEventListener("hashchange", check);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}

export function App() {
  const { status, nav } = useApp();
  useEmailVerificationLink();
  if (!status) return <Spinner />;
  if (!status.session) return <Login />;
  if (status.account?.totpRequired) return <TotpRequired />;
  if (status.settings["onboarding.completed"] !== "true") return <Onboarding />;
  const r = nav.route;
  // an invited funeral office works only in the shared calendar
  if (status.session.role === "undertaker") {
    return (
      <Shell>
        <OperationsView />
      </Shell>
    );
  }
  return (
    <Shell>
      {r.view === "dashboard" && <Dashboard />}
      {r.view === "burials" && <BurialsList />}
      {r.view === "newBurial" && <NewBurial key={(r.plotId ?? "") + (r.eventType ?? "") + (r.bookingId ?? "")} plotId={r.plotId} eventType={r.eventType} bookingId={r.bookingId} />}
      {r.view === "deceased" && <DeceasedView key={r.id} id={r.id} />}
      {r.view === "plots" && <PlotsList sectionId={r.sectionId} />}
      {r.view === "plot" && <PlotDetailView key={r.id} id={r.id} />}
      {r.view === "map" && <MapView plotId={r.plotId} />}
      {r.view === "persons" && <PersonsList />}
      {r.view === "person" && <PersonView key={r.id} id={r.id} />}
      {r.view === "expiries" && <Expiries />}
      {r.view === "prints" && <Prints />}
      {r.view === "reports" && <Reports />}
      {r.view === "importExport" && <ImportExport />}
      {r.view === "quickEntry" && <QuickEntry />}
      {r.view === "settings" && <SettingsView key={r.tab ?? ""} tab={r.tab} />}
      {r.view === "platform" && <PlatformAdmin />}
      {r.view === "finance" && <FinanceView />}
      {r.view === "operations" && <OperationsView />}
      {r.view === "help" && <Help />}
    </Shell>
  );
}
