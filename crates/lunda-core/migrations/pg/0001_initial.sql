-- TEMISZ – tenant schema v1 (Postgres). Each customer has its own schema;
-- the connection's search_path points at it. Same conventions as the
-- desktop schema: UUID text ids, ISO date strings, integer HUF, no deletes.

CREATE TABLE settings (
    key          TEXT NOT NULL,
    cemetery_id  TEXT NOT NULL DEFAULT '',   -- '' = global
    value        TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    updated_by   TEXT,
    PRIMARY KEY (key, cemetery_id)
);

CREATE TABLE users (
    id               TEXT PRIMARY KEY,
    username         TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    role             TEXT NOT NULL CHECK (role IN ('admin','clerk','finance','reader')),
    extra_permissions TEXT NOT NULL DEFAULT '[]',  -- JSON array of permission codes
    password_hash    TEXT NOT NULL,
    active           BIGINT NOT NULL DEFAULT 1,
    failed_attempts  BIGINT NOT NULL DEFAULT 0,
    locked_until     TEXT,
    must_change_password BIGINT NOT NULL DEFAULT 0,
    last_login_at    TEXT,
    created_at       TEXT NOT NULL,
    created_by       TEXT,
    updated_at       TEXT NOT NULL,
    updated_by       TEXT
);

CREATE UNIQUE INDEX ux_users_username ON users (lower(username));

CREATE TABLE cemeteries (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    address       TEXT NOT NULL DEFAULT '',
    parcel_number TEXT NOT NULL DEFAULT '',   -- helyrajzi szám
    kind          TEXT NOT NULL DEFAULT 'public' CHECK (kind IN ('public','denominational')),
    owner         TEXT NOT NULL DEFAULT '',
    operator      TEXT NOT NULL DEFAULT '',
    opening_hours TEXT NOT NULL DEFAULT '',
    folio_scope   TEXT NOT NULL DEFAULT 'continuous' CHECK (folio_scope IN ('continuous','yearly')),
    notes         TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

CREATE TABLE plot_types (
    id                 TEXT PRIMARY KEY,
    code               TEXT NOT NULL UNIQUE,
    name               TEXT NOT NULL,
    category           TEXT NOT NULL CHECK (category IN ('grave','crypt','urn','scatter','common')),
    default_lease_years BIGINT NOT NULL,
    rest_period_years  BIGINT NOT NULL,
    coffin_capacity    BIGINT NOT NULL,
    urn_capacity       BIGINT NOT NULL,
    width_cm           BIGINT,
    length_cm          BIGINT,
    builtin            BIGINT NOT NULL DEFAULT 0,
    sort_order         BIGINT NOT NULL DEFAULT 100,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

CREATE TABLE sections (
    id           TEXT PRIMARY KEY,
    cemetery_id  TEXT NOT NULL REFERENCES cemeteries(id),
    code         TEXT NOT NULL,               -- e.g. "A"
    name         TEXT NOT NULL DEFAULT '',
    map_x DOUBLE PRECISION NOT NULL DEFAULT 0, map_y DOUBLE PRECISION NOT NULL DEFAULT 0,
    map_w DOUBLE PRECISION NOT NULL DEFAULT 0, map_h DOUBLE PRECISION NOT NULL DEFAULT 0,
    map_rot DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE UNIQUE INDEX ux_sections_code ON sections(cemetery_id, code) WHERE voided_at IS NULL;

CREATE TABLE plot_rows (
    id          TEXT PRIMARY KEY,
    section_id  TEXT NOT NULL REFERENCES sections(id),
    number      TEXT NOT NULL,
    direction   TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE UNIQUE INDEX ux_rows_number ON plot_rows(section_id, number) WHERE voided_at IS NULL;

CREATE TABLE plots (
    id                TEXT PRIMARY KEY,
    cemetery_id       TEXT NOT NULL REFERENCES cemeteries(id),
    section_id        TEXT NOT NULL REFERENCES sections(id),
    row_id            TEXT NOT NULL REFERENCES plot_rows(id),
    number            TEXT NOT NULL,
    identifier        TEXT NOT NULL,               -- "A/3/12"
    plot_type_id      TEXT NOT NULL REFERENCES plot_types(id),
    coffin_capacity   BIGINT NOT NULL,
    urn_capacity      BIGINT NOT NULL,
    width_cm          BIGINT,
    length_cm         BIGINT,
    rest_period_years BIGINT,                      -- NULL = from plot type
    status_override   TEXT,                         -- NULL = computed
    status_override_reason TEXT,
    unusable          BIGINT NOT NULL DEFAULT 0,   -- e.g. groundwater, road
    protected         BIGINT NOT NULL DEFAULT 0,
    protection_kind   TEXT CHECK (protection_kind IN ('national','local') OR protection_kind IS NULL),
    protection_reason TEXT,
    further_burial_allowed BIGINT NOT NULL DEFAULT 1,
    resale_ready_at   TEXT,                         -- set when confirmed "újraértékesíthető"
    notes             TEXT NOT NULL DEFAULT '',
    map_x DOUBLE PRECISION NOT NULL DEFAULT 0, map_y DOUBLE PRECISION NOT NULL DEFAULT 0,
    map_w DOUBLE PRECISION NOT NULL DEFAULT 0, map_h DOUBLE PRECISION NOT NULL DEFAULT 0,
    map_rot DOUBLE PRECISION NOT NULL DEFAULT 0,
    merged_into_id    TEXT REFERENCES plots(id),
    gps_lat DOUBLE PRECISION, gps_lon DOUBLE PRECISION,
    -- crypt book fields (FR-SIR-01)
    crypt_number      TEXT,
    crypt_built_year  BIGINT,
    crypt_permit      TEXT,
    crypt_holder_instructions TEXT,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE UNIQUE INDEX ux_plots_identifier ON plots(cemetery_id, identifier) WHERE voided_at IS NULL;
CREATE INDEX ix_plots_row ON plots(row_id);
CREATE INDEX ix_plots_section ON plots(section_id);

CREATE TABLE crypt_slots (
    id        TEXT PRIMARY KEY,
    plot_id   TEXT NOT NULL REFERENCES plots(id),
    slot_no   BIGINT NOT NULL,
    label     TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT,
    UNIQUE (plot_id, slot_no)
);

CREATE TABLE plot_restrictions (
    id         TEXT PRIMARY KEY,
    plot_id    TEXT NOT NULL REFERENCES plots(id),
    kind       TEXT NOT NULL CHECK (kind IN ('ban','condition')),
    text       TEXT NOT NULL,
    valid_from TEXT,
    valid_to   TEXT,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_restrictions_plot ON plot_restrictions(plot_id);

CREATE TABLE deceased (
    id              TEXT PRIMARY KEY,
    name_prefix     TEXT NOT NULL DEFAULT '',     -- dr., özv.
    family_name     TEXT NOT NULL DEFAULT '',
    given_name      TEXT NOT NULL DEFAULT '',
    birth_name      TEXT NOT NULL DEFAULT '',
    sex             TEXT NOT NULL DEFAULT '' CHECK (sex IN ('','male','female','unknown')),
    birth_place     TEXT NOT NULL DEFAULT '',
    birth_date      TEXT,
    mother_name     TEXT NOT NULL DEFAULT '',
    last_residence  TEXT NOT NULL DEFAULT '',
    death_place     TEXT NOT NULL DEFAULT '',
    death_date      TEXT,
    death_cert_no   TEXT NOT NULL DEFAULT '',
    unknown_identity BIGINT NOT NULL DEFAULT 0,
    authority_name  TEXT NOT NULL DEFAULT '',
    case_number     TEXT NOT NULL DEFAULT '',
    notes           TEXT NOT NULL DEFAULT '',
    search_text     TEXT NOT NULL DEFAULT '',     -- normalized (no accents, lowercase)
    sort_key        TEXT NOT NULL DEFAULT '',     -- text::sort_key(family + given)
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_deceased_birth ON deceased(birth_date);
CREATE INDEX ix_deceased_death ON deceased(death_date);
CREATE INDEX ix_deceased_sort ON deceased(sort_key);

CREATE TABLE persons (
    id            TEXT PRIMARY KEY,
    name_prefix   TEXT NOT NULL DEFAULT '',
    family_name   TEXT NOT NULL DEFAULT '',
    given_name    TEXT NOT NULL DEFAULT '',
    address       TEXT NOT NULL DEFAULT '',
    mail_address  TEXT NOT NULL DEFAULT '',
    phone         TEXT NOT NULL DEFAULT '',
    email         TEXT NOT NULL DEFAULT '',
    notes         TEXT NOT NULL DEFAULT '',
    search_text   TEXT NOT NULL DEFAULT '',
    sort_key      TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

CREATE INDEX ix_persons_sort ON persons(sort_key);

CREATE TABLE register_sequences (
    cemetery_id TEXT NOT NULL REFERENCES cemeteries(id),
    scope_key   TEXT NOT NULL,         -- 'all' or the year, e.g. '2026'
    next_value  BIGINT NOT NULL,
    PRIMARY KEY (cemetery_id, scope_key)
);

CREATE TABLE interment_events (
    id              TEXT PRIMARY KEY,
    cemetery_id     TEXT NOT NULL REFERENCES cemeteries(id),
    scope_key       TEXT NOT NULL,
    folio           BIGINT NOT NULL,
    historical      BIGINT NOT NULL DEFAULT 0,   -- entered from a paper book with its original folio
    event_type      TEXT NOT NULL CHECK (event_type IN (
                       'coffin_burial','urn_placement','reburial','urn_release',
                       'urn_removal','ash_scattering','relocation','common_grave')),
    container       TEXT NOT NULL DEFAULT 'coffin' CHECK (container IN ('coffin','urn','none')),
    event_date      TEXT NOT NULL,
    deceased_id     TEXT NOT NULL REFERENCES deceased(id),
    plot_id         TEXT REFERENCES plots(id),
    crypt_slot_id   TEXT REFERENCES crypt_slots(id),
    arranger_id     TEXT REFERENCES persons(id),     -- eltemettető
    receiver_name   TEXT NOT NULL DEFAULT '',        -- urn release: átvevő
    receiver_address TEXT NOT NULL DEFAULT '',
    destination     TEXT NOT NULL DEFAULT '',        -- relocation / release destination
    source_event_id TEXT REFERENCES interment_events(id), -- event this one removes (urn removal, relocation)
    override_reason TEXT,                            -- set when a burial check was overridden
    notes           TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT,
    UNIQUE (cemetery_id, scope_key, folio)
);
CREATE INDEX ix_events_plot ON interment_events(plot_id);
CREATE INDEX ix_events_deceased ON interment_events(deceased_id);
CREATE INDEX ix_events_date ON interment_events(event_date);
CREATE INDEX ix_events_source ON interment_events(source_event_id);
CREATE INDEX ix_events_slot ON interment_events(crypt_slot_id);

CREATE TABLE plot_holders (
    id           TEXT PRIMARY KEY,
    plot_id      TEXT NOT NULL REFERENCES plots(id),
    person_id    TEXT NOT NULL REFERENCES persons(id),
    role         TEXT NOT NULL CHECK (role IN ('primary','contact')),
    relation     TEXT NOT NULL DEFAULT '',        -- kapcsolat az elhunyttal
    start_date   TEXT NOT NULL,
    end_date     TEXT,
    legal_basis  TEXT NOT NULL DEFAULT '',        -- jogcím: megváltás, öröklés, átruházás
    document_ref TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_holders_plot ON plot_holders(plot_id);
CREATE INDEX ix_holders_person ON plot_holders(person_id);

CREATE TABLE fee_rates (
    id            TEXT PRIMARY KEY,
    cemetery_id   TEXT NOT NULL REFERENCES cemeteries(id),
    plot_type_id  TEXT REFERENCES plot_types(id),
    fee_kind      TEXT NOT NULL,     -- purchase, renewal, opening, chapel, maintenance ...
    amount_huf    BIGINT NOT NULL,
    valid_from    TEXT NOT NULL,
    valid_to      TEXT,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

CREATE TABLE leases (
    id             TEXT PRIMARY KEY,
    plot_id        TEXT NOT NULL REFERENCES plots(id),
    person_id      TEXT REFERENCES persons(id),
    kind           TEXT NOT NULL CHECK (kind IN ('purchase','prepurchase','renewal')),
    start_date     TEXT NOT NULL,
    duration_years BIGINT NOT NULL,
    expiry_date    TEXT NOT NULL,
    fee_huf        BIGINT NOT NULL DEFAULT 0,
    fee_rate_id    TEXT REFERENCES fee_rates(id),
    fee_exempt     BIGINT NOT NULL DEFAULT 0,
    status         TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','superseded','cancelled')),
    cancel_date    TEXT,
    refund_huf     BIGINT,
    related_event_id TEXT REFERENCES interment_events(id),
    notes          TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_leases_plot ON leases(plot_id);
CREATE INDEX ix_leases_expiry ON leases(expiry_date);
-- invariant: at most one active lease per plot (previous ones are 'superseded')
CREATE UNIQUE INDEX ux_leases_active ON leases(plot_id) WHERE status = 'active' AND voided_at IS NULL;

CREATE TABLE notices (
    id          TEXT PRIMARY KEY,
    plot_id     TEXT NOT NULL REFERENCES plots(id),
    lease_id    TEXT REFERENCES leases(id),
    kind        TEXT NOT NULL CHECK (kind IN ('letter','email','label','board')),
    notice_date TEXT NOT NULL,
    result      TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_notices_plot ON notices(plot_id);

CREATE TABLE monuments (
    id          TEXT PRIMARY KEY,
    plot_id     TEXT NOT NULL REFERENCES plots(id),
    entry_date  TEXT NOT NULL,
    description TEXT NOT NULL,
    contractor  TEXT NOT NULL DEFAULT '',
    material    TEXT NOT NULL DEFAULT '',
    size        TEXT NOT NULL DEFAULT '',
    permit_date TEXT,
    protected   BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);
CREATE INDEX ix_monuments_plot ON monuments(plot_id);

-- Prepared for v1.0 (FR-ELH-06), schema only
CREATE TABLE external_placements (
    id              TEXT PRIMARY KEY,
    deceased_id     TEXT NOT NULL REFERENCES deceased(id),
    kind            TEXT NOT NULL CHECK (kind IN ('acceptance','outside_urn')),
    placement_place TEXT NOT NULL,
    declarant_name  TEXT NOT NULL,
    declarant_address TEXT NOT NULL,
    declaration_date TEXT NOT NULL,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

-- Prepared for v1.0 (FR-ELH-07), schema only
CREATE TABLE attachments (
    id          TEXT PRIMARY KEY,
    entity      TEXT NOT NULL,
    entity_id   TEXT NOT NULL,
    file_name   TEXT NOT NULL,
    mime_type   TEXT NOT NULL,
    sha256      TEXT NOT NULL,
    content     BYTEA NOT NULL,
    created_at TEXT NOT NULL, created_by TEXT, updated_at TEXT NOT NULL, updated_by TEXT,
    voided_at TEXT, voided_by TEXT, void_reason TEXT
);

CREATE TABLE audit_log (
    seq        BIGSERIAL PRIMARY KEY,
    ts         TEXT NOT NULL,
    user_id    TEXT,
    user_name  TEXT NOT NULL,
    entity     TEXT NOT NULL,
    entity_id  TEXT NOT NULL,
    action     TEXT NOT NULL,
    old_value  TEXT,
    new_value  TEXT,
    reason     TEXT,
    prev_hash  TEXT NOT NULL,
    hash       TEXT NOT NULL UNIQUE
);
CREATE INDEX ix_audit_entity ON audit_log(entity, entity_id);

CREATE TABLE print_log (
    id           TEXT PRIMARY KEY,
    ts           TEXT NOT NULL,
    user_name    TEXT NOT NULL,
    document     TEXT NOT NULL,
    parameters   TEXT NOT NULL,
    page_count   BIGINT NOT NULL,
    content_hash TEXT NOT NULL
);

CREATE TABLE backup_log (
    id         TEXT PRIMARY KEY,
    ts         TEXT NOT NULL,
    kind       TEXT NOT NULL,
    path       TEXT NOT NULL,
    sha256     TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    external   BIGINT NOT NULL DEFAULT 0,
    ok         BIGINT NOT NULL,
    message    TEXT NOT NULL DEFAULT ''
);

-- Full-text search over deceased and persons. `text` holds the
-- accent-folded, lower-case form produced by text::normalize(); the trigram
-- tokenizer makes substring LIKE '%..%' queries index-assisted.
CREATE TABLE search_index (
    kind   TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    text   TEXT NOT NULL
);
CREATE INDEX ix_search_ref ON search_index(ref_id);
CREATE INDEX ix_search_text ON search_index USING gin (text gin_trgm_ops);

-- ---------------------------------------------------------------------------
-- Integrity triggers (JOG-04, FR-ELH-03, FR-NAP-01, FR-NAP-02)
-- ---------------------------------------------------------------------------

CREATE FUNCTION forbid() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '%', TG_ARGV[0];
END $$;

CREATE FUNCTION forbid_folio_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.folio IS DISTINCT FROM OLD.folio OR NEW.scope_key IS DISTINCT FROM OLD.scope_key
       OR NEW.cemetery_id IS DISTINCT FROM OLD.cemetery_id THEN
        RAISE EXCEPTION 'folio is immutable';
    END IF;
    IF OLD.voided_at IS NOT NULL THEN
        RAISE EXCEPTION 'voided record is final';
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log FOR EACH ROW EXECUTE FUNCTION forbid('audit_log is append-only');
CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log FOR EACH ROW EXECUTE FUNCTION forbid('audit_log is append-only');
CREATE TRIGGER print_log_no_update BEFORE UPDATE ON print_log FOR EACH ROW EXECUTE FUNCTION forbid('print_log is append-only');
CREATE TRIGGER print_log_no_delete BEFORE DELETE ON print_log FOR EACH ROW EXECUTE FUNCTION forbid('print_log is append-only');
CREATE TRIGGER no_delete_cemeteries BEFORE DELETE ON cemeteries FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_sections BEFORE DELETE ON sections FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_rows BEFORE DELETE ON plot_rows FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_plots BEFORE DELETE ON plots FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_slots BEFORE DELETE ON crypt_slots FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_restrictions BEFORE DELETE ON plot_restrictions FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_deceased BEFORE DELETE ON deceased FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_persons BEFORE DELETE ON persons FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_events BEFORE DELETE ON interment_events FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_holders BEFORE DELETE ON plot_holders FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_leases BEFORE DELETE ON leases FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_notices BEFORE DELETE ON notices FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_monuments BEFORE DELETE ON monuments FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_fee_rates BEFORE DELETE ON fee_rates FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; void instead');
CREATE TRIGGER no_delete_users BEFORE DELETE ON users FOR EACH ROW EXECUTE FUNCTION forbid('physical delete forbidden; deactivate instead');
CREATE TRIGGER events_immutable BEFORE UPDATE ON interment_events FOR EACH ROW EXECUTE FUNCTION forbid_folio_change();
