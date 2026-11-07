-- Security log: sign-ins (also failed ones), password and two-factor
-- changes, with IP address and browser. Kept 1 year.
CREATE TABLE platform.security_log (
    id BIGSERIAL PRIMARY KEY,
    tenant_id TEXT,
    account_id TEXT,
    email TEXT NOT NULL,
    kind TEXT NOT NULL,
    ip TEXT NOT NULL DEFAULT '',
    user_agent TEXT NOT NULL DEFAULT '',
    at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_security_log_tenant ON platform.security_log (tenant_id, at DESC);
