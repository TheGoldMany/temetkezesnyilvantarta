-- Subscriptions per cemetery. An organisation is free; each cemetery has its
-- own subscription:
--   card:     the card is checked first (pending_card), then 15 days of trial,
--             then charged automatically every period until cancelled;
--   transfer: no trial, a proforma at once, the cemetery works while it is open;
--   demo:     the sample cemetery, free, no data export, watermarked prints.
-- A cemetery without a usable subscription can only be read.
CREATE TABLE platform.cemetery_subscriptions (
    tenant_id            TEXT NOT NULL REFERENCES platform.tenants(id),
    cemetery_id          TEXT NOT NULL,
    cemetery_name        TEXT NOT NULL,
    plan                 TEXT NOT NULL CHECK (plan IN ('monthly', 'yearly')),
    payment_method       TEXT NOT NULL CHECK (payment_method IN ('card', 'transfer', 'demo')),
    status               TEXT NOT NULL CHECK (status IN ('pending_card', 'trial', 'active', 'past_due', 'canceled', 'lapsed', 'demo')),
    trial_ends_at        TIMESTAMPTZ,
    current_period_start DATE,
    current_period_end   DATE,                 -- exclusive
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT false,
    recurrence_id        TEXT,                 -- Barion card token
    trace_id             TEXT,                 -- Barion 3DS trace of the card check
    failed_charges       INT NOT NULL DEFAULT 0,
    past_due_since       DATE,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, cemetery_id)
);
CREATE INDEX ix_cemsub_status ON platform.cemetery_subscriptions (status);

ALTER TABLE platform.invoices ADD COLUMN cemetery_id TEXT;
ALTER TABLE platform.card_payments ADD COLUMN cemetery_id TEXT;

-- Existing customers keep what they had, cemetery by cemetery:
-- a paid subscription stays paid for its period, a running trial keeps its
-- end date (without a card it then needs a proforma), a read-only account's
-- cemeteries are lapsed.
DO $$
DECLARE
    t RECORD;
    c RECORD;
    s RECORD;
BEGIN
    FOR t IN SELECT id, schema_name, status, trial_ends_at FROM platform.tenants WHERE status <> 'closed' AND deleted_at IS NULL LOOP
        IF to_regclass(format('%I.cemeteries', t.schema_name)) IS NULL THEN
            CONTINUE;
        END IF;
        SELECT * INTO s FROM platform.subscriptions WHERE tenant_id = t.id;
        FOR c IN EXECUTE format('SELECT id, name FROM %I.cemeteries WHERE voided_at IS NULL', t.schema_name) LOOP
            INSERT INTO platform.cemetery_subscriptions
                (tenant_id, cemetery_id, cemetery_name, plan, payment_method, status, trial_ends_at, current_period_start, current_period_end, cancel_at_period_end)
            VALUES (
                t.id, c.id, c.name,
                coalesce(s.plan, 'monthly'),
                'transfer',
                CASE
                    WHEN s.status IN ('active', 'pending') THEN 'active'
                    WHEN s.status = 'past_due' THEN 'past_due'
                    WHEN s.status = 'canceled' THEN 'canceled'
                    WHEN t.status = 'trial' AND t.trial_ends_at > now() THEN 'trial'
                    ELSE 'lapsed'
                END,
                t.trial_ends_at,
                s.current_period_start,
                s.current_period_end,
                coalesce(s.cancel_at_period_end, false)
            )
            ON CONFLICT DO NOTHING;
        END LOOP;
    END LOOP;
END $$;

-- the organisation itself is free from now on
UPDATE platform.tenants SET status = 'active', read_only_since = NULL WHERE status IN ('trial', 'read_only');
