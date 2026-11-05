import { useApp } from "./app/context";
import { Shell } from "./app/Shell";
import { Spinner } from "./components/ui";
import { Login } from "./features/auth/Login";
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

export function App() {
  const { status, nav } = useApp();
  if (!status) return <Spinner />;
  if (!status.session) return <Login />;
  if (status.settings["onboarding.completed"] !== "true") return <Onboarding />;
  const r = nav.route;
  return (
    <Shell>
      {r.view === "dashboard" && <Dashboard />}
      {r.view === "burials" && <BurialsList />}
      {r.view === "newBurial" && <NewBurial key={(r.plotId ?? "") + (r.eventType ?? "")} plotId={r.plotId} eventType={r.eventType} />}
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
      {r.view === "settings" && <SettingsView tab={r.tab} />}
    </Shell>
  );
}
