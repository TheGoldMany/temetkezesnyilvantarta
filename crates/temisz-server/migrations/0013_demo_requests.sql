-- Free migration demo requests from temisz.hu/bemutato: the uploaded table
-- (personal data) is stored encrypted and deleted after 30 days.
CREATE TABLE platform.demo_requests (
    id           TEXT PRIMARY KEY,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    settlement   TEXT NOT NULL,
    contact_name TEXT NOT NULL,
    contact_role TEXT NOT NULL DEFAULT '',
    email        TEXT NOT NULL,
    phone        TEXT NOT NULL DEFAULT '',
    message      TEXT NOT NULL DEFAULT '',
    file_name    TEXT NOT NULL,
    file_size    BIGINT NOT NULL,
    file_data    BYTEA NOT NULL,
    sealed       BOOLEAN NOT NULL,
    ip           TEXT NOT NULL DEFAULT '',
    done_at      TIMESTAMPTZ
);
CREATE INDEX ix_demo_requests_created ON platform.demo_requests (created_at);
