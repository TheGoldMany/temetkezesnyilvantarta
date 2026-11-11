// Hungarian formatting helpers (NFR-HAS-01).

const dateFmt = new Intl.DateTimeFormat("hu-HU", { year: "numeric", month: "2-digit", day: "2-digit" });
const dateTimeFmt = new Intl.DateTimeFormat("hu-HU", { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
const hufFmt = new Intl.NumberFormat("hu-HU", { style: "currency", currency: "HUF", maximumFractionDigits: 0 });
const numFmt = new Intl.NumberFormat("hu-HU");

/** Hungarian alphabetical order (cs, dz, dzs, gy, ly, ny, sz, ty, zs). */
export const huCollator = new Intl.Collator("hu", { sensitivity: "base", numeric: true });

function parse(iso: string): Date {
  // dates are 'YYYY-MM-DD' – parse as local date, not UTC
  if (/^\d{4}-\d{2}-\d{2}$/.test(iso)) {
    const [y, m, d] = iso.split("-").map(Number);
    return new Date(y, m - 1, d);
  }
  return new Date(iso);
}

export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "–";
  const d = parse(iso);
  return Number.isNaN(d.getTime()) ? iso : dateFmt.format(d);
}

export function fmtDateTime(iso: string | null | undefined): string {
  if (!iso) return "–";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : dateTimeFmt.format(d);
}

export function fmtHuf(n: number | null | undefined): string {
  if (n === null || n === undefined) return "–";
  return hufFmt.format(n);
}

export function fmtNum(n: number): string {
  return numFmt.format(n);
}

export function todayIso(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function year(iso: string | null | undefined): string {
  return iso ? iso.slice(0, 4) : "?";
}

/** Hungarian name order: "dr. Kovács János". */
export function fullName(p: { namePrefix?: string; familyName?: string; givenName?: string }): string {
  return [p.namePrefix, p.familyName, p.givenName].map((s) => (s ?? "").trim()).filter(Boolean).join(" ");
}

/** Accepts "2026.01.05", "2026-01-05", "20260105", "2026. 01. 05." */
export function normalizeDateInput(v: string): string {
  const digits = v.replace(/[^\d]/g, "");
  if (digits.length === 8) return `${digits.slice(0, 4)}-${digits.slice(4, 6)}-${digits.slice(6, 8)}`;
  return v.trim();
}
