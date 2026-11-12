import { useApp } from "./app/context";
import { Spinner } from "./components/ui";
import { lazy, Suspense, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Login } from "./features/auth/Login";
import { TotpRequired } from "./features/account/Security";
import { command, shell } from "./lib/api";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "./app/toast";
import { Analytics } from "@vercel/analytics/react";

/** Pages load on first use: the sign-in page does not carry the whole program. */
const Onboarding = lazy(() => import("./features/auth/Onboarding").then((m) => ({ default: m.Onboarding })));
const Shell = lazy(() => import("./app/Shell").then((m) => ({ default: m.Shell })));
const PlatformAdmin = lazy(() => import("./features/account/PlatformAdmin").then((m) => ({ default: m.PlatformAdmin })));
const FinanceView = lazy(() => import("./features/finance/Finance").then((m) => ({ default: m.FinanceView })));
const OperationsView = lazy(() => import("./features/operations/Operations").then((m) => ({ default: m.OperationsView })));
const Help = lazy(() => import("./features/help/Help").then((m) => ({ default: m.Help })));
const Dashboard = lazy(() => import("./features/dashboard/Dashboard").then((m) => ({ default: m.Dashboard })));
const BurialsList = lazy(() => import("./features/burials/BurialsList").then((m) => ({ default: m.BurialsList })));
const NewBurial = lazy(() => import("./features/burials/NewBurial").then((m) => ({ default: m.NewBurial })));
const DeceasedView = lazy(() => import("./features/burials/DeceasedView").then((m) => ({ default: m.DeceasedView })));
const PlotsList = lazy(() => import("./features/plots/PlotsList").then((m) => ({ default: m.PlotsList })));
const PlotDetailView = lazy(() => import("./features/plots/PlotDetail").then((m) => ({ default: m.PlotDetailView })));
const Expiries = lazy(() => import("./features/expiries/Expiries").then((m) => ({ default: m.Expiries })));
const MapView = lazy(() => import("./features/map/MapView").then((m) => ({ default: m.MapView })));
const Prints = lazy(() => import("./features/prints/Prints").then((m) => ({ default: m.Prints })));
const Reports = lazy(() => import("./features/reports/Reports").then((m) => ({ default: m.Reports })));
const ImportExport = lazy(() => import("./features/io/ImportExport").then((m) => ({ default: m.ImportExport })));
const QuickEntry = lazy(() => import("./features/io/QuickEntry").then((m) => ({ default: m.QuickEntry })));
const PersonsList = lazy(() => import("./features/persons/Persons").then((m) => ({ default: m.PersonsList })));
const PersonView = lazy(() => import("./features/persons/Persons").then((m) => ({ default: m.PersonView })));
const AccountView = lazy(() => import("./features/settings/Settings").then((m) => ({ default: m.AccountView })));
const SettingsView = lazy(() => import("./features/settings/Settings").then((m) => ({ default: m.SettingsView })));

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

/** #fizetes=… : back from the Barion card payment page. Barion is asked for
 *  the payment's state; the Előfizetés page shows the result. */
function useCardPaymentReturn() {
  const { status, nav, refresh } = useApp();
  const toast = useToast();
  const { t } = useTranslation();
  const qc = useQueryClient();
  const signedIn = Boolean(status?.session);
  useEffect(() => {
    const h = window.location.hash;
    // #fizetes=: a proforma paid by card; #kartya=: the card check of a new subscription
    const check = h.startsWith("#kartya=");
    if (!signedIn || !(h.startsWith("#fizetes=") || check)) return;
    const requestId = h.slice(h.indexOf("=") + 1);
    history.replaceState(null, "", window.location.pathname + window.location.search);
    nav.go({ view: "settings", tab: "subscription" });
    command<{ status: string }>("billing_card_status", { requestId }).then(
      async ({ status: state }) => {
        // the page shows the settled proforma before the message appears
        await qc.invalidateQueries({ queryKey: ["billing"] });
        await refresh();
        if (state === "Succeeded") toast(check ? t("billing.cardChecked") : t("billing.cardPaid"));
        else if (["Canceled", "Failed", "Expired", "Rejected"].includes(state)) toast(t("billing.cardFailed"), "error");
        else toast(t("billing.cardPending"));
      },
      () => toast(t("billing.cardFailed"), "error"),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signedIn]);
}

export function App() {
  const { status, nav } = useApp();
  useEmailVerificationLink();
  useCardPaymentReturn();
  if (!status) return <Spinner />;
  if (!status.session) return <Login />;
  if (status.account?.totpRequired) return <TotpRequired />;
  if (status.settings["onboarding.completed"] !== "true")
    return (
      <Suspense fallback={<Spinner />}>
        <Onboarding />
      </Suspense>
    );
  const r = nav.route;
  // an invited funeral office works only in the shared calendar
  if (status.session.role === "undertaker") {
    return (
      <>
        <Suspense fallback={<Spinner />}>
          <Shell>
            <Suspense fallback={<Spinner />}>
              <OperationsView />
            </Suspense>
          </Shell>
        </Suspense>
        <Analytics />
      </>
    );
  }
  return (
    <>
      <Suspense fallback={<Spinner />}>
        <Shell>
          <Suspense fallback={<Spinner />}>
            {r.view === "dashboard" && <Dashboard />}
            {r.view === "burials" && <BurialsList />}
            {r.view === "newBurial" && (
              <NewBurial key={(r.plotId ?? "") + (r.eventType ?? "") + (r.bookingId ?? "")} plotId={r.plotId} eventType={r.eventType} bookingId={r.bookingId} />
            )}
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
            {r.view === "settings" && (r.tab === "account" ? <AccountView /> : <SettingsView key={r.tab ?? ""} tab={r.tab} />)}
            {r.view === "account" && <AccountView />}
            {r.view === "platform" && <PlatformAdmin />}
            {r.view === "finance" && <FinanceView key={r.tab ?? ""} tab={r.tab} />}
            {r.view === "operations" && <OperationsView />}
            {r.view === "help" && <Help />}
          </Suspense>
        </Shell>
      </Suspense>
      <Analytics />
    </>
  );
}
