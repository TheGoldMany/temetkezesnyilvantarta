import { useApp } from "../app/context";

/** Barion's official payment-methods banner, unmodified (their acceptance rules): the dark variant on the dark theme. */
export function BarionBanner({ className }: { className?: string }) {
  const { theme } = useApp();
  return (
    <img
      src={theme === "dark" ? "/barion-banner-dark.svg" : "/barion-banner-light.svg"}
      alt="Barion – elfogadott fizetési módok: bankkártya, Apple Pay, Google Pay"
      width={567}
      height={108}
      className={className ?? "block h-10 w-auto max-w-full"}
    />
  );
}
