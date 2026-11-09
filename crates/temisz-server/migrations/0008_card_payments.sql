-- Card payments of proformas through Barion: one row per started payment;
-- the state is always the one Barion reports.
CREATE TABLE platform.card_payments (
    payment_id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    tenant_id TEXT NOT NULL,
    invoice_id TEXT NOT NULL,
    gross_huf BIGINT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_card_payments_tenant ON platform.card_payments (tenant_id, created_at DESC);
