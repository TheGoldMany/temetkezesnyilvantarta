import { describe, expect, it } from "vitest";
import { buildGrid, queryGrid } from "./spatial";

describe("spatial grid", () => {
  const items = Array.from({ length: 50_000 }, (_, i) => ({ id: `p${i}`, x: (i % 250) * 15, y: Math.floor(i / 250) * 28, w: 10, h: 20 }));
  const grid = buildGrid(items);

  it("returns only plots near the viewport", () => {
    const hits = queryGrid(grid, { x: 0, y: 0, w: 300, h: 200 });
    expect(hits.has("p0")).toBe(true);
    expect(hits.has("p49999")).toBe(false);
    expect(hits.size).toBeLessThan(1000);
  });

  it("queries 50 000 plots fast", () => {
    const t = performance.now();
    for (let i = 0; i < 100; i++) queryGrid(grid, { x: i * 10, y: i * 10, w: 1200, h: 800 });
    expect(performance.now() - t).toBeLessThan(500);
  });
});
