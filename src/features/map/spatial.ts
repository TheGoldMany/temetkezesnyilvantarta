// Uniform grid spatial index for viewport culling (50 000 plots, NFR-TEL-02).

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Grid {
  cell: number;
  cells: Map<string, string[]>;
}

const CELL = 120; // world units (dm) – about 8 graves per cell side

export function buildGrid(items: (Box & { id: string })[], cell = CELL): Grid {
  const cells = new Map<string, string[]>();
  for (const it of items) {
    const cx0 = Math.floor(it.x / cell);
    const cy0 = Math.floor(it.y / cell);
    const cx1 = Math.floor((it.x + it.w) / cell);
    const cy1 = Math.floor((it.y + it.h) / cell);
    for (let cx = cx0; cx <= cx1; cx++)
      for (let cy = cy0; cy <= cy1; cy++) {
        const k = `${cx},${cy}`;
        const arr = cells.get(k);
        if (arr) arr.push(it.id);
        else cells.set(k, [it.id]);
      }
  }
  return { cell, cells };
}

/** Ids of items whose cells intersect the box (may include near misses). */
export function queryGrid(g: Grid, b: Box): Set<string> {
  const out = new Set<string>();
  const cx0 = Math.floor(b.x / g.cell);
  const cy0 = Math.floor(b.y / g.cell);
  const cx1 = Math.floor((b.x + b.w) / g.cell);
  const cy1 = Math.floor((b.y + b.h) / g.cell);
  if ((cx1 - cx0 + 1) * (cy1 - cy0 + 1) > 40_000) return out; // absurd zoom-out guard
  for (let cx = cx0; cx <= cx1; cx++)
    for (let cy = cy0; cy <= cy1; cy++) {
      const arr = g.cells.get(`${cx},${cy}`);
      if (arr) for (const id of arr) out.add(id);
    }
  return out;
}
