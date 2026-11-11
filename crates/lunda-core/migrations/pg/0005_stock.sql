-- Goods for sale at the office (candles, wreaths, plates, urns) and
-- materials: items, every movement of stock, and stocktakes.
CREATE TABLE stock_items (
    id            TEXT PRIMARY KEY,
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    sku           TEXT NOT NULL DEFAULT '',
    name          TEXT NOT NULL,
    unit          TEXT NOT NULL DEFAULT 'db',
    unit_net_huf  BIGINT NOT NULL DEFAULT 0,          -- sale price, net
    vat_rate      TEXT NOT NULL DEFAULT '27',
    min_qty       BIGINT NOT NULL DEFAULT 0,          -- below this: reorder
    for_sale      BOOLEAN NOT NULL DEFAULT true,      -- false: material, only used
    active        BOOLEAN NOT NULL DEFAULT true,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT
);
CREATE INDEX ix_stock_items_cemetery ON stock_items (cemetery_id, active);

-- in: goods received; sale: sold; use: used (work order, repair); adjust: stocktake difference
CREATE TABLE stock_moves (
    id            TEXT PRIMARY KEY,
    item_id       TEXT NOT NULL REFERENCES stock_items(id),
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    kind          TEXT NOT NULL CHECK (kind IN ('in', 'sale', 'use', 'adjust')),
    qty           BIGINT NOT NULL,                    -- signed: + in, − out
    unit_cost_huf BIGINT,                             -- purchase price (in)
    unit_net_huf  BIGINT,                             -- sale price (sale)
    partner       TEXT NOT NULL DEFAULT '',
    note          TEXT NOT NULL DEFAULT '',
    cash_entry_id TEXT,
    charge_id     TEXT,
    stocktake_id  TEXT,
    moved_on      TEXT NOT NULL,
    created_at TEXT NOT NULL, created_by TEXT
);
CREATE INDEX ix_stock_moves_item ON stock_moves (item_id, moved_on);

CREATE TABLE stocktakes (
    id            TEXT PRIMARY KEY,
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    taken_on      TEXT NOT NULL,
    note          TEXT NOT NULL DEFAULT '',
    lines         TEXT NOT NULL DEFAULT '[]',         -- [{itemId, name, expected, counted}]
    created_at TEXT NOT NULL, created_by TEXT
);
