-- One person, several organisations: an account row is a membership of an
-- e-mail address in one tenant. The sign-in data (password, second factor,
-- confirmed address) is kept the same on every membership of the address.
DROP INDEX platform.ux_accounts_email;
CREATE UNIQUE INDEX ux_accounts_email_tenant ON platform.accounts (lower(email), tenant_id);
CREATE INDEX ix_accounts_email ON platform.accounts (lower(email));
ALTER TABLE platform.accounts ADD COLUMN last_used_at TIMESTAMPTZ;
