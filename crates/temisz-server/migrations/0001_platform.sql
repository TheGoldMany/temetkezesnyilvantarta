-- TEMISZ platform schema: customers (tenants), sign-in accounts, sessions.
-- Register data lives in each tenant's own schema (t_<id>), never here.

CREATE TABLE platform.tenants (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    schema_name   TEXT NOT NULL UNIQUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    trial_ends_at TIMESTAMPTZ NOT NULL,
    status        TEXT NOT NULL DEFAULT 'trial' CHECK (status IN ('trial', 'active', 'read_only', 'closed')),
    tax_number    TEXT NOT NULL DEFAULT '',
    billing_email TEXT NOT NULL DEFAULT ''
);

CREATE TABLE platform.accounts (
    id                TEXT PRIMARY KEY,
    email             TEXT NOT NULL,
    tenant_id         TEXT NOT NULL REFERENCES platform.tenants(id),
    user_id           TEXT NOT NULL,          -- users.id inside the tenant schema
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    email_verified_at TIMESTAMPTZ
);
CREATE UNIQUE INDEX ux_accounts_email ON platform.accounts (lower(email));
CREATE INDEX ix_accounts_tenant ON platform.accounts (tenant_id);

CREATE TABLE platform.sessions (
    token_hash   TEXT PRIMARY KEY,            -- SHA-256 of the cookie value
    account_id   TEXT NOT NULL REFERENCES platform.accounts(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    ip           TEXT NOT NULL DEFAULT '',
    user_agent   TEXT NOT NULL DEFAULT ''
);
CREATE INDEX ix_sessions_account ON platform.sessions (account_id);
