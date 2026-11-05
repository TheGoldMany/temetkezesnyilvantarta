import { describe, expect, it } from "vitest";
import { split } from "./names";
import { huCollator, normalizeDateInput } from "../../lib/format";

describe("Hungarian names and order", () => {
  it("splits prefixes", () => {
    expect(split("dr. Kovács János Péter")).toEqual(["dr.", "Kovács", "János Péter"]);
    expect(split("özv. Nagy Istvánné")).toEqual(["özv.", "Nagy", "Istvánné"]);
    expect(split("Kiss")).toEqual(["", "Kiss", ""]);
  });
  it("sorts with the Hungarian collator", () => {
    const names = ["Csonka", "Cukor", "Zsolt", "Zoltán", "Ács", "Adorján"];
    expect([...names].sort(huCollator.compare)).toEqual(["Ács", "Adorján", "Cukor", "Csonka", "Zoltán", "Zsolt"]);
  });
  it("normalizes typed dates", () => {
    expect(normalizeDateInput("20260105")).toBe("2026-01-05");
    expect(normalizeDateInput("2026.01.05.")).toBe("2026-01-05");
  });
});
