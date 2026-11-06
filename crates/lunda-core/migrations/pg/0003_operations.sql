-- W8–W11: funeral calendar, contractors, work orders, case files, cemetery
-- inspections (docs/FRS-UZEMELTETES.md FR-SZE, FR-VAL, FR-ENG, FR-UZE).
-- Times are local ISO strings 'YYYY-MM-DDTHH:MM' (Europe/Budapest).

-- FR-SZE-01: chapel, cooler, hall … that can be booked
CREATE TABLE facilities (
    id          TEXT PRIMARY KEY,
    cemetery_id TEXT NOT NULL REFERENCES cemeteries(id),
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'chapel' CHECK (kind IN ('chapel', 'cooler', 'hall', 'other')),
    notes       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

-- FR-VAL-01: funeral directors, stonemasons, gardeners
CREATE TABLE contractors (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL DEFAULT 'funeral' CHECK (kind IN ('funeral', 'stonemason', 'gardener', 'other')),
    tax_number    TEXT NOT NULL DEFAULT '',
    postcode      TEXT NOT NULL DEFAULT '',
    city          TEXT NOT NULL DEFAULT '',
    address       TEXT NOT NULL DEFAULT '',
    contact_name  TEXT NOT NULL DEFAULT '',
    phone         TEXT NOT NULL DEFAULT '',
    email         TEXT NOT NULL DEFAULT '',
    permit_number TEXT NOT NULL DEFAULT '',
    permit_valid_until TEXT,
    notes         TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

-- FR-SZE-01/02: funeral appointments and other bookings
CREATE TABLE bookings (
    id            TEXT PRIMARY KEY,
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    title         TEXT NOT NULL,
    starts_at     TEXT NOT NULL,
    ends_at       TEXT NOT NULL,
    facility_id   TEXT REFERENCES facilities(id),
    plot_id       TEXT REFERENCES plots(id),
    contractor_id TEXT REFERENCES contractors(id),
    deceased_name TEXT NOT NULL DEFAULT '',
    container     TEXT NOT NULL DEFAULT '' CHECK (container IN ('', 'coffin', 'urn')),
    event_id      TEXT REFERENCES interment_events(id),     -- set when the burial is recorded
    status        TEXT NOT NULL DEFAULT 'planned' CHECK (status IN ('planned', 'done', 'cancelled')),
    notes         TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    CHECK (ends_at > starts_at)
);
CREATE INDEX ix_bookings_time ON bookings (cemetery_id, starts_at);
CREATE INDEX ix_bookings_facility ON bookings (facility_id, starts_at);

-- FR-VAL-02: work announced by a contractor (with the maintenance contribution fee)
CREATE TABLE work_notices (
    id            TEXT PRIMARY KEY,
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    contractor_id TEXT NOT NULL REFERENCES contractors(id),
    plot_id       TEXT REFERENCES plots(id),
    kind          TEXT NOT NULL CHECK (kind IN ('burial', 'monument', 'renovation', 'demolition', 'other')),
    work_date     TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    charge_id     TEXT REFERENCES charges(id),
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_work_notices_date ON work_notices (cemetery_id, work_date);

-- FR-SZE-03, FR-UZE-01: work orders (grave digging, maintenance), optionally recurring
CREATE TABLE work_orders (
    id          TEXT PRIMARY KEY,
    cemetery_id TEXT NOT NULL REFERENCES cemeteries(id),
    kind        TEXT NOT NULL CHECK (kind IN ('grave_digging', 'preparation', 'maintenance', 'cleaning', 'tree', 'repair', 'other')),
    title       TEXT NOT NULL,
    plot_id     TEXT REFERENCES plots(id),
    booking_id  TEXT REFERENCES bookings(id),
    due_date    TEXT NOT NULL,
    assignee    TEXT NOT NULL DEFAULT '',
    details     TEXT NOT NULL DEFAULT '',              -- depth, coffin/urn, etc.
    recurrence  TEXT NOT NULL DEFAULT 'none' CHECK (recurrence IN ('none', 'weekly', 'monthly', 'yearly')),
    status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done', 'cancelled')),
    done_at     TEXT,
    done_by     TEXT,
    done_note   TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT
);
CREATE INDEX ix_work_orders_due ON work_orders (cemetery_id, status, due_date);

-- FR-ENG-01/02: case files with gap-free filing numbers per cemetery and year
CREATE TABLE case_sequences (
    cemetery_id TEXT NOT NULL REFERENCES cemeteries(id),
    year        TEXT NOT NULL,
    next_value  BIGINT NOT NULL,
    PRIMARY KEY (cemetery_id, year)
);

CREATE TABLE cases (
    id             TEXT PRIMARY KEY,
    cemetery_id    TEXT NOT NULL REFERENCES cemeteries(id),
    case_number    TEXT NOT NULL,                      -- 12/2026
    kind           TEXT NOT NULL CHECK (kind IN ('lease_contract', 'renewal', 'monument_permit', 'monument_removal', 'exhumation',
                                                 'relocation', 'ash_release', 'holder_transfer', 'neglect', 'complaint', 'other')),
    subject        TEXT NOT NULL,
    applicant_id   TEXT REFERENCES persons(id),
    applicant_name TEXT NOT NULL DEFAULT '',
    plot_id        TEXT REFERENCES plots(id),
    contractor_id  TEXT REFERENCES contractors(id),
    received_on    TEXT NOT NULL,
    deadline       TEXT,
    status         TEXT NOT NULL DEFAULT 'received' CHECK (status IN ('received', 'missing_docs', 'approved', 'rejected', 'closed')),
    decision       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT,
    UNIQUE (cemetery_id, case_number)
);
CREATE INDEX ix_cases_status ON cases (cemetery_id, status, deadline);
CREATE INDEX ix_cases_plot ON cases (plot_id);

-- FR-UZE-02: plot condition seen during an inspection walk
CREATE TABLE plot_conditions (
    id          TEXT PRIMARY KEY,
    plot_id     TEXT NOT NULL REFERENCES plots(id),
    observed_on TEXT NOT NULL,
    condition   TEXT NOT NULL CHECK (condition IN ('ok', 'neglected', 'dangerous', 'to_liquidate')),
    note        TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT
);
CREATE INDEX ix_plot_conditions_plot ON plot_conditions (plot_id, observed_on);

-- FR-UGY-02: deceased hidden from the public grave search on request
ALTER TABLE deceased ADD COLUMN hide_public BIGINT NOT NULL DEFAULT 0;

CREATE TRIGGER no_delete_facilities BEFORE DELETE ON facilities FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_contractors BEFORE DELETE ON contractors FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_bookings BEFORE DELETE ON bookings FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_work_notices BEFORE DELETE ON work_notices FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_work_orders BEFORE DELETE ON work_orders FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_cases BEFORE DELETE ON cases FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_plot_conditions BEFORE DELETE ON plot_conditions FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');

-- FR-UZE-01: field worker role (sees and closes work orders on the phone)
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check;
ALTER TABLE users ADD CONSTRAINT users_role_check CHECK (role IN ('admin', 'clerk', 'finance', 'reader', 'cashier', 'worker'));
