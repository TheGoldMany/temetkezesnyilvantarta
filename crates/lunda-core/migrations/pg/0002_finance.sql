-- W7: finance and cash desk (docs/FRS-UZEMELTETES.md, FR-PEN-01..10).
-- Money is integer HUF. Nothing is deleted: voided or reversed (storno).

-- FR-PEN-01: fee items from the cemetery's fee decree
ALTER TABLE fee_rates
    ADD COLUMN name        TEXT NOT NULL DEFAULT '',
    ADD COLUMN unit        TEXT NOT NULL DEFAULT 'db',
    ADD COLUMN vat_rate    TEXT NOT NULL DEFAULT '27' CHECK (vat_rate IN ('27', '18', '5', '0', 'AAM', 'TAM', 'EUFAD37', 'KBAET', 'ATK')),
    ADD COLUMN ledger_code TEXT NOT NULL DEFAULT '';

-- buyers: people already in the register get billing fields
ALTER TABLE persons
    ADD COLUMN tax_number TEXT NOT NULL DEFAULT '',
    ADD COLUMN postcode   TEXT NOT NULL DEFAULT '',
    ADD COLUMN city       TEXT NOT NULL DEFAULT '';

-- FR-PEN-02: something to be paid (from an event, a lease, a permit or by hand)
CREATE TABLE charges (
    id            TEXT PRIMARY KEY,
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    person_id     TEXT REFERENCES persons(id),          -- payer
    payer_name    TEXT NOT NULL,
    fee_rate_id   TEXT REFERENCES fee_rates(id),
    description   TEXT NOT NULL,
    quantity      BIGINT NOT NULL DEFAULT 1,
    unit          TEXT NOT NULL DEFAULT 'db',
    unit_net_huf  BIGINT NOT NULL,
    vat_rate      TEXT NOT NULL DEFAULT '27',
    net_huf       BIGINT NOT NULL,
    vat_huf       BIGINT NOT NULL,
    gross_huf     BIGINT NOT NULL,
    ledger_code   TEXT NOT NULL DEFAULT '',
    entity        TEXT,                                 -- plot | lease | event | permit | work
    entity_id     TEXT,
    status        TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'invoiced', 'exported', 'void')),
    invoice_id    TEXT,
    override_reason TEXT,                               -- price changed by hand
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_charges_status ON charges (cemetery_id, status);
CREATE INDEX ix_charges_person ON charges (person_id);
CREATE INDEX ix_charges_entity ON charges (entity, entity_id);

-- FR-PEN-03/04: invoices issued through Számlázz.hu, or prepared for the ASP
CREATE TABLE invoices (
    id            TEXT PRIMARY KEY,
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    kind          TEXT NOT NULL CHECK (kind IN ('invoice', 'proforma', 'storno')),
    provider      TEXT NOT NULL CHECK (provider IN ('szamlazz', 'asp')),
    number        TEXT NOT NULL DEFAULT '',             -- empty until the ASP number is recorded
    storno_of     TEXT REFERENCES invoices(id),
    person_id     TEXT REFERENCES persons(id),
    buyer_name    TEXT NOT NULL,
    buyer_address TEXT NOT NULL DEFAULT '',
    buyer_tax_number TEXT NOT NULL DEFAULT '',
    buyer_email   TEXT NOT NULL DEFAULT '',
    issue_date    TEXT NOT NULL,
    fulfilment_date TEXT NOT NULL,
    due_date      TEXT NOT NULL,
    payment_method TEXT NOT NULL CHECK (payment_method IN ('cash', 'transfer', 'card')),
    net_huf       BIGINT NOT NULL,
    vat_huf       BIGINT NOT NULL,
    gross_huf     BIGINT NOT NULL,
    paid_huf      BIGINT NOT NULL DEFAULT 0,
    status        TEXT NOT NULL DEFAULT 'issued' CHECK (status IN ('pending_asp', 'issued', 'paid', 'partially_paid', 'storno')),
    pdf           BYTEA,
    provider_log  TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT
);
CREATE INDEX ix_invoices_status ON invoices (cemetery_id, status, due_date);
CREATE INDEX ix_invoices_person ON invoices (person_id);

CREATE TABLE invoice_lines (
    id           TEXT PRIMARY KEY,
    invoice_id   TEXT NOT NULL REFERENCES invoices(id),
    charge_id    TEXT REFERENCES charges(id),
    description  TEXT NOT NULL,
    quantity     BIGINT NOT NULL,
    unit         TEXT NOT NULL,
    unit_net_huf BIGINT NOT NULL,
    vat_rate     TEXT NOT NULL,
    net_huf      BIGINT NOT NULL,
    vat_huf      BIGINT NOT NULL,
    gross_huf    BIGINT NOT NULL,
    ledger_code  TEXT NOT NULL DEFAULT ''
);
CREATE INDEX ix_invoice_lines_invoice ON invoice_lines (invoice_id);

-- FR-PEN-05: money received (or refunded, negative)
CREATE TABLE payments (
    id           TEXT PRIMARY KEY,
    cemetery_id  TEXT NOT NULL REFERENCES cemeteries(id),
    invoice_id   TEXT REFERENCES invoices(id),
    person_id    TEXT REFERENCES persons(id),
    payer_name   TEXT NOT NULL,
    amount_huf   BIGINT NOT NULL,
    method       TEXT NOT NULL CHECK (method IN ('cash', 'transfer', 'card')),
    paid_on      TEXT NOT NULL,
    reference    TEXT NOT NULL DEFAULT '',             -- receipt / bank reference
    cash_entry_id TEXT,
    bank_tx_id   TEXT,
    notes        TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_payments_invoice ON payments (invoice_id);
CREATE INDEX ix_payments_date ON payments (cemetery_id, paid_on);

-- FR-PEN-06: cash desk with gap-free receipt numbers per cemetery and year
CREATE TABLE cash_sequences (
    cemetery_id TEXT NOT NULL REFERENCES cemeteries(id),
    year        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('in', 'out')),
    next_value  BIGINT NOT NULL,
    PRIMARY KEY (cemetery_id, year, kind)
);

CREATE TABLE cash_entries (
    id           TEXT PRIMARY KEY,
    cemetery_id  TEXT NOT NULL REFERENCES cemeteries(id),
    entry_date   TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('in', 'out')),
    doc_number   TEXT NOT NULL,                        -- BE-2026/0001, KI-2026/0001
    amount_huf   BIGINT NOT NULL CHECK (amount_huf > 0),
    partner_name TEXT NOT NULL,
    description  TEXT NOT NULL,
    payment_id   TEXT REFERENCES payments(id),
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT,
    UNIQUE (cemetery_id, doc_number)
);
CREATE INDEX ix_cash_entries_date ON cash_entries (cemetery_id, entry_date);

CREATE TABLE cash_closings (
    cemetery_id     TEXT NOT NULL REFERENCES cemeteries(id),
    closing_date    TEXT NOT NULL,
    opening_huf     BIGINT NOT NULL,
    in_huf          BIGINT NOT NULL,
    out_huf         BIGINT NOT NULL,
    closing_huf     BIGINT NOT NULL,
    counted_huf     BIGINT NOT NULL,                   -- cash counted in the drawer
    difference_note TEXT NOT NULL DEFAULT '',
    closed_at TEXT NOT NULL, closed_by TEXT,
    PRIMARY KEY (cemetery_id, closing_date)
);

-- closed days cannot change
CREATE FUNCTION cash_day_open() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE d TEXT; c TEXT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'physical delete forbidden; void instead';
    END IF;
    d := NEW.entry_date; c := NEW.cemetery_id;
    IF EXISTS (SELECT 1 FROM cash_closings WHERE cemetery_id = c AND closing_date >= d) THEN
        RAISE EXCEPTION 'cash day is closed';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER cash_entries_open_day BEFORE INSERT OR UPDATE OR DELETE ON cash_entries FOR EACH ROW EXECUTE FUNCTION cash_day_open();

-- FR-PEN-07: bank statement lines to match with payments
CREATE TABLE bank_transactions (
    id          TEXT PRIMARY KEY,
    cemetery_id TEXT NOT NULL REFERENCES cemeteries(id),
    booked_on   TEXT NOT NULL,
    amount_huf  BIGINT NOT NULL,
    partner     TEXT NOT NULL DEFAULT '',
    reference   TEXT NOT NULL DEFAULT '',
    fingerprint TEXT NOT NULL,                          -- avoids importing a line twice
    status      TEXT NOT NULL DEFAULT 'unmatched' CHECK (status IN ('unmatched', 'matched', 'ignored')),
    payment_id  TEXT REFERENCES payments(id),
    created_at TEXT NOT NULL, created_by TEXT,
    UNIQUE (cemetery_id, fingerprint)
);

-- FR-PEN-08: payment reminders sent
CREATE TABLE reminders (
    id          TEXT PRIMARY KEY,
    invoice_id  TEXT NOT NULL REFERENCES invoices(id),
    sent_on     TEXT NOT NULL,
    channel     TEXT NOT NULL CHECK (channel IN ('letter', 'email')),
    level       BIGINT NOT NULL,
    created_at TEXT NOT NULL, created_by TEXT
);
CREATE INDEX ix_reminders_invoice ON reminders (invoice_id);

CREATE TRIGGER no_delete_charges BEFORE DELETE ON charges FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_invoices BEFORE DELETE ON invoices FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_invoice_lines BEFORE DELETE ON invoice_lines FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_payments BEFORE DELETE ON payments FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_cash_closings BEFORE DELETE ON cash_closings FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER cash_closings_final BEFORE UPDATE ON cash_closings FOR EACH ROW EXECUTE FUNCTION forbid('cash closing is final');

-- FR-PEN-10: cashier role (cash desk only)
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check;
ALTER TABLE users ADD CONSTRAINT users_role_check CHECK (role IN ('admin', 'clerk', 'finance', 'reader', 'cashier'));
