import { createContext, useCallback, useContext, useState, type ReactNode } from "react";
import { CheckCircle2, AlertTriangle } from "lucide-react";

interface Toast {
  id: number;
  text: string;
  tone: "ok" | "error";
}

const ToastCtx = createContext<(text: string, tone?: "ok" | "error") => void>(() => {});

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Toast[]>([]);
  const push = useCallback((text: string, tone: "ok" | "error" = "ok") => {
    const id = Date.now() + Math.random();
    setItems((s) => [...s, { id, text, tone }]);
    setTimeout(() => setItems((s) => s.filter((t) => t.id !== id)), tone === "error" ? 9000 : 5000);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="no-print pointer-events-none fixed right-6 bottom-6 z-50 flex flex-col gap-3" aria-live="polite">
        {items.map((t) => (
          <div
            key={t.id}
            className={`pointer-events-auto flex max-w-md items-start gap-3 rounded-md border px-4 py-3 ${t.tone === "ok" ? "border-moss/50 bg-slab" : "border-alert/50 bg-alert-bg text-alert"}`}
            role={t.tone === "error" ? "alert" : "status"}
          >
            {t.tone === "ok" ? <CheckCircle2 className="mt-0.5 size-5 text-moss" aria-hidden /> : <AlertTriangle className="mt-0.5 size-5" aria-hidden />}
            <span>{t.text}</span>
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

export function useToast() {
  return useContext(ToastCtx);
}
