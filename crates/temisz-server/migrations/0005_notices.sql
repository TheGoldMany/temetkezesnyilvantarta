-- W12: e-mails sent on behalf of an operator are logged with the tenant
-- (FR-UGY-01 delivery log); the public grave search is reached by a token
-- that does not reveal the tenant (FR-UGY-02).
ALTER TABLE platform.emails ADD COLUMN tenant_id TEXT;
CREATE INDEX ix_emails_tenant ON platform.emails (tenant_id, created_at);
ALTER TABLE platform.tenants ADD COLUMN public_token TEXT UNIQUE;
