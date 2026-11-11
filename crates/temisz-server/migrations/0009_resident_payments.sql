-- Online card payment of an operator's invoices by the payer (resident),
-- into the operator's own Barion wallet. The operator's Barion key is stored
-- encrypted; a payment link is an unguessable token for one invoice.
ALTER TABLE platform.card_payments ADD COLUMN kind TEXT NOT NULL DEFAULT 'subscription';
ALTER TABLE platform.card_payments ADD COLUMN pay_token TEXT;
CREATE TABLE platform.tenant_barion (
    tenant_id TEXT PRIMARY KEY,
    pos_key_enc BYTEA NOT NULL,
    payee TEXT NOT NULL,
    live BOOLEAN NOT NULL DEFAULT false,
    terms_url TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by TEXT NOT NULL DEFAULT ''
);
CREATE TABLE platform.pay_links (
    token TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    invoice_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, invoice_id)
);
