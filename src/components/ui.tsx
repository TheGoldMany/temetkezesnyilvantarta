import { forwardRef, useEffect, useId, useRef, useState, type ReactNode, type InputHTMLAttributes, type SelectHTMLAttributes, type ButtonHTMLAttributes, type TextareaHTMLAttributes } from "react";
import { useTranslation } from "react-i18next";
import { X, AlertTriangle, Info, CheckCircle2, Loader2 } from "lucide-react";
import type { PlotStatus } from "../lib/types";
import { ApiError } from "../lib/api";

export function cx(...c: (string | false | null | undefined)[]) {
  return c.filter(Boolean).join(" ");
}

// ---------------------------------------------------------------- buttons

type Variant = "primary" | "secondary" | "quiet" | "danger";
const variants: Record<Variant, string> = {
  primary: "bg-moss text-moss-ink border-moss hover:brightness-110",
  secondary: "bg-slab text-slate border-seam hover:border-granite",
  quiet: "bg-transparent text-slate border-transparent hover:bg-lichen/60",
  danger: "bg-slab text-alert border-alert/50 hover:bg-alert-bg",
};

export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; icon?: ReactNode; busy?: boolean; size?: "md" | "lg" }>(
  ({ variant = "secondary", icon, busy, size = "md", className, children, disabled, ...rest }, ref) => (
    <button
      ref={ref}
      type="button"
      disabled={disabled || busy}
      className={cx(
        "inline-flex items-center justify-center gap-2 rounded-md border font-semibold transition-[filter,background-color,border-color] disabled:opacity-50 disabled:cursor-not-allowed",
        size === "lg" ? "min-h-13 px-5 text-lg" : "min-h-11 px-4",
        variants[variant],
        className,
      )}
      {...rest}
    >
      {busy ? <Loader2 className="size-5 animate-spin" aria-hidden /> : icon}
      {children}
    </button>
  ),
);
Button.displayName = "Button";

// ---------------------------------------------------------------- form fields

export function Field({ label, hint, error, required, children, className, htmlFor }: { label: ReactNode; hint?: ReactNode; error?: string | null; required?: boolean; children: ReactNode; className?: string; htmlFor?: string }) {
  const { t } = useTranslation();
  const text = (
    <span className="font-semibold">
      {label}
      {required && <span className="ml-1 font-normal text-granite">({t("common.required")})</span>}
    </span>
  );
  const extra = (
    <>
      {hint && !error && <span className="text-sm text-granite">{hint}</span>}
      {error && (
        <span className="text-sm font-semibold text-alert" role="alert">
          {error}
        </span>
      )}
    </>
  );
  // Without an explicit id the label wraps the control (implicit association, WCAG 1.3.1).
  if (!htmlFor) {
    return (
      <label className={cx("flex flex-col gap-1", className)}>
        {text}
        {children}
        {extra}
      </label>
    );
  }
  return (
    <div className={cx("flex flex-col gap-1", className)}>
      <label htmlFor={htmlFor}>{text}</label>
      {children}
      {extra}
    </div>
  );
}

const inputCls =
  "min-h-11 w-full rounded-md border border-seam bg-slab px-3 py-2 text-slate placeholder:text-granite/80 focus:border-moss focus:outline-none focus-visible:outline-3 focus-visible:outline-moss aria-invalid:border-alert";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(({ className, ...rest }, ref) => (
  <input ref={ref} className={cx(inputCls, className)} {...rest} />
));
Input.displayName = "Input";

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(({ className, ...rest }, ref) => (
  <textarea ref={ref} className={cx(inputCls, "min-h-20", className)} {...rest} />
));
Textarea.displayName = "Textarea";

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(({ className, children, ...rest }, ref) => (
  <select ref={ref} className={cx(inputCls, "pr-8", className)} {...rest}>
    {children}
  </select>
));
Select.displayName = "Select";

/**
 * Hungarian date field (ÉÉÉÉ.HH.NN.) – typed with the keyboard, independent of
 * the OS/browser locale. Accepts "2026.01.05", "2026-01-05", "20260105".
 * The value passed in/out is ISO 'YYYY-MM-DD' (or the raw text while incomplete).
 */
export const DateInput = forwardRef<HTMLInputElement, Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> & { value: string | null | undefined; onValue: (iso: string) => void }>(
  ({ value, onValue, className, ...rest }, ref) => {
    const toDisplay = (v: string | null | undefined) => (v && /^\d{4}-\d{2}-\d{2}$/.test(v) ? `${v.slice(0, 4)}.${v.slice(5, 7)}.${v.slice(8, 10)}.` : (v ?? ""));
    const [text, setText] = useState(toDisplay(value));
    useEffect(() => {
      setText((cur) => (toIso(cur) === (value ?? "") ? cur : toDisplay(value)));
    }, [value]);
    return (
      <input
        ref={ref}
        inputMode="numeric"
        placeholder="ÉÉÉÉ.HH.NN."
        className={cx(inputCls, "tabular-nums", className)}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          onValue(toIso(e.target.value));
        }}
        onBlur={() => setText(toDisplay(toIso(text)))}
        {...rest}
      />
    );
  },
);
DateInput.displayName = "DateInput";

function toIso(v: string): string {
  const d = v.replace(/[^\d]/g, "");
  if (d.length === 8) return `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6, 8)}`;
  return v.trim() === "" ? "" : v;
}

/** Labelled text input in one go. */
export function TextField({ label, value, onChange, hint, error, required, type = "text", className, inputProps }: {
  label: ReactNode;
  value: string | number | null | undefined;
  onChange: (v: string) => void;
  hint?: ReactNode;
  error?: string | null;
  required?: boolean;
  type?: string;
  className?: string;
  inputProps?: InputHTMLAttributes<HTMLInputElement>;
}) {
  const id = useId();
  return (
    <Field label={label} hint={hint} error={error} required={required} className={className} htmlFor={id}>
      {type === "date" ? (
        <DateInput id={id} value={value == null ? "" : String(value)} onValue={onChange} aria-invalid={!!error} aria-required={required} {...(inputProps as Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange">)} />
      ) : (
        <Input id={id} type={type} value={value ?? ""} onChange={(e) => onChange(e.target.value)} aria-invalid={!!error} aria-required={required} {...inputProps} />
      )}
    </Field>
  );
}

export function Checkbox({ label, checked, onChange, hint, disabled }: { label: ReactNode; checked: boolean; onChange: (v: boolean) => void; hint?: ReactNode; disabled?: boolean }) {
  const id = useId();
  return (
    <div className="flex items-start gap-3">
      <input id={id} type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} className="mt-1 size-5 accent-[var(--c-moss)]" />
      <label htmlFor={id} className="cursor-pointer">
        <span className="font-semibold">{label}</span>
        {hint && <span className="block text-sm text-granite">{hint}</span>}
      </label>
    </div>
  );
}

export function RadioGroup<T extends string>({ label, value, onChange, options }: { label: ReactNode; value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; hint?: ReactNode }[] }) {
  const name = useId();
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-1 font-semibold">{label}</legend>
      {options.map((o) => (
        <label key={o.value} className={cx("flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2", value === o.value ? "border-moss bg-lichen/50" : "border-seam bg-slab")}>
          <input type="radio" name={name} checked={value === o.value} onChange={() => onChange(o.value)} className="mt-1 size-5 accent-[var(--c-moss)]" />
          <span>
            <span className="font-semibold">{o.label}</span>
            {o.hint && <span className="block text-sm text-granite">{o.hint}</span>}
          </span>
        </label>
      ))}
    </fieldset>
  );
}

// ---------------------------------------------------------------- plaque & status

/** The plot identifier plaque – the signature element (docs/DESIGN.md). */
export function Plaque({ identifier, large, className }: { identifier: string | null | undefined; large?: boolean; className?: string }) {
  if (!identifier) return <span className="text-granite">–</span>;
  const parts = identifier.split("/");
  return (
    <span className={cx("plaque", large && "plaque-lg", className)} aria-label={identifier}>
      {parts.map((p, i) => (
        <span key={i} className="contents">
          {i > 0 && <span className="sep" aria-hidden>/</span>}
          <span>{p}</span>
        </span>
      ))}
    </span>
  );
}

export const statusColor: Record<PlotStatus, string> = {
  free: "var(--st-free)",
  reserved: "#6e93bf",
  occupied: "#4f7a60",
  expired: "#c9962e",
  notified: "#a65a2a",
  resellable: "#7d62a0",
  liquidated: "#7c8486",
  unusable: "#3d4447",
};

export function StatusDot({ status, protectedPlot }: { status: PlotStatus; protectedPlot?: boolean }) {
  return (
    <span
      aria-hidden
      className={cx("inline-block size-3.5 shrink-0 rounded-full border", status === "unusable" && "pattern-hatch", status === "notified" && "pattern-hatch")}
      style={{ background: statusColor[status], borderColor: protectedPlot ? "#b08d2f" : status === "free" ? "var(--c-granite)" : "transparent", borderWidth: protectedPlot ? 2 : 1 }}
    />
  );
}

export function StatusBadge({ status, protectedPlot }: { status: PlotStatus; protectedPlot?: boolean }) {
  const { t } = useTranslation();
  return (
    <span className="inline-flex items-center gap-2 whitespace-nowrap">
      <StatusDot status={status} protectedPlot={protectedPlot} />
      <span>{t(`status.${status}`)}</span>
      {protectedPlot && <span className="text-sm text-granite">· {t("status.protected")}</span>}
    </span>
  );
}

// ---------------------------------------------------------------- layout bits

export function PageHeader({ title, subtitle, actions, back }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode; back?: ReactNode }) {
  return (
    <header className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        {back}
        <h1 className="text-2xl">{title}</h1>
        {subtitle && <p className="mt-1 text-granite">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap gap-3">{actions}</div>}
    </header>
  );
}

export function Panel({ title, actions, children, className }: { title?: ReactNode; actions?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={cx("rounded-lg border border-seam bg-slab", className)}>
      {(title || actions) && (
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-seam px-5 py-3">
          {title && <h2 className="text-lg">{title}</h2>}
          {actions && <div className="flex flex-wrap gap-2">{actions}</div>}
        </div>
      )}
      <div className="px-5 py-4">{children}</div>
    </section>
  );
}

export function Alert({ tone = "info", title, children }: { tone?: "info" | "warn" | "error" | "ok"; title?: ReactNode; children?: ReactNode }) {
  const cls = {
    info: "border-seam bg-slab",
    warn: "border-warn/50 bg-warn-bg text-warn",
    error: "border-alert/50 bg-alert-bg text-alert",
    ok: "border-moss/40 bg-lichen/50",
  }[tone];
  const Icon = tone === "ok" ? CheckCircle2 : tone === "info" ? Info : AlertTriangle;
  return (
    <div className={cx("flex gap-3 rounded-md border px-4 py-3", cls)} role={tone === "error" ? "alert" : "status"}>
      <Icon className="mt-0.5 size-5 shrink-0" aria-hidden />
      <div className="min-w-0">
        {title && <p className="font-semibold">{title}</p>}
        {children && <div className={cx(!!title && "mt-1")}>{children}</div>}
      </div>
    </div>
  );
}

export function Empty({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-3 rounded-lg border border-dashed border-seam px-6 py-8 text-granite">
      <p>{children}</p>
      {action}
    </div>
  );
}

export function Spinner({ label }: { label?: string }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-3 py-6 text-granite" role="status">
      <Loader2 className="size-5 animate-spin" aria-hidden /> {label ?? t("common.loading")}
    </div>
  );
}

export function DefinitionList({ items }: { items: [ReactNode, ReactNode][] }) {
  return (
    <dl className="grid grid-cols-[minmax(10rem,auto)_1fr] gap-x-6 gap-y-2">
      {items.map(([k, v], i) => (
        <div key={i} className="contents">
          <dt className="text-granite">{k}</dt>
          <dd className="m-0 min-w-0 break-words">{v ?? "–"}</dd>
        </div>
      ))}
    </dl>
  );
}

// ---------------------------------------------------------------- dialog

export function Dialog({ open, onClose, title, children, footer, wide }: { open: boolean; onClose: () => void; title: ReactNode; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  const ref = useRef<HTMLDialogElement>(null);
  const { t } = useTranslation();
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      className={cx("m-auto max-h-[90vh] w-[calc(100%-2rem)] rounded-lg border border-seam bg-slab p-0 text-slate backdrop:bg-slate/40", wide ? "max-w-4xl" : "max-w-xl")}
    >
      {open && (
        <div className="flex max-h-[90vh] flex-col">
          <div className="flex items-center justify-between gap-4 border-b border-seam px-6 py-4">
            <h2 className="text-xl">{title}</h2>
            <Button variant="quiet" onClick={onClose} aria-label={t("common.close")} className="min-h-10 px-2">
              <X className="size-5" />
            </Button>
          </div>
          <div className="overflow-y-auto px-6 py-5">{children}</div>
          {footer && <div className="flex flex-wrap justify-end gap-3 border-t border-seam px-6 py-4">{footer}</div>}
        </div>
      )}
    </dialog>
  );
}

// ---------------------------------------------------------------- errors

export function useErrorText() {
  const { t } = useTranslation();
  return (e: unknown): string => {
    if (e instanceof ApiError) {
      if (e.code === "invalid_credentials" && e.params.remaining) return t("errors.invalid_credentials_remaining", e.params);
      if (e.code === "validation" && e.params.field) {
        const field = t(`fields.${e.params.field}`, { defaultValue: e.params.field });
        if (e.params.code && t(`errors.${e.params.code}`, { defaultValue: "" })) return `${field}: ${t(`errors.${e.params.code}`)}`;
        return `${field}: ${t("common.required")}`;
      }
      const key = `errors.${e.code}`;
      const txt = t(key, { ...e.params, defaultValue: "" });
      return txt || t("errors.generic", { message: e.message });
    }
    return t("errors.generic", { message: String(e) });
  };
}

export function ErrorAlert({ error }: { error: unknown }) {
  const text = useErrorText();
  if (!error) return null;
  return <Alert tone="error">{text(error)}</Alert>;
}

export function Tabs<T extends string>({ value, onChange, items }: { value: T; onChange: (v: T) => void; items: { value: T; label: ReactNode }[] }) {
  return (
    <div role="tablist" className="flex flex-wrap gap-1 border-b border-seam">
      {items.map((it) => (
        <button
          key={it.value}
          role="tab"
          type="button"
          aria-selected={value === it.value}
          onClick={() => onChange(it.value)}
          className={cx("-mb-px min-h-11 rounded-t-md border px-4 font-semibold", value === it.value ? "border-seam border-b-slab bg-slab text-slate" : "border-transparent text-granite hover:text-slate")}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}
