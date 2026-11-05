import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "./styles.css";
import "./i18n";
import { App } from "./App";
import { AppProvider } from "./app/context";
import { ToastProvider } from "./app/toast";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false, staleTime: 10_000 } },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <AppProvider>
        <ToastProvider>
          <App />
        </ToastProvider>
      </AppProvider>
    </QueryClientProvider>
  </StrictMode>,
);
