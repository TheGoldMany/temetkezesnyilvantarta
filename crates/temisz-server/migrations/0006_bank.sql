-- Bank statement lines of TEMISZ's own account, matched to customer proformas
-- (transfer is the only payment method).
CREATE TABLE platform.bank_lines (
    id          TEXT PRIMARY KEY,
    booked_on   DATE NOT NULL,
    amount_huf  BIGINT NOT NULL,
    partner     TEXT NOT NULL DEFAULT '',
    reference   TEXT NOT NULL DEFAULT '',
    fingerprint TEXT NOT NULL UNIQUE,           -- a statement imported twice adds nothing
    status      TEXT NOT NULL DEFAULT 'unmatched' CHECK (status IN ('unmatched', 'matched', 'ignored')),
    invoice_id  TEXT REFERENCES platform.invoices(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    matched_at  TIMESTAMPTZ,
    matched_by  TEXT
);
CREATE INDEX ix_bank_lines_status ON platform.bank_lines (status, booked_on);
