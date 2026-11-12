-- Subscriptions, second pass (same release as 0012):
-- * paid_at: the first payment arrived. Until then nothing can be exported
--   from the cemetery (a transfer order without payment is not a way to get
--   the register out).
-- * the card check and the renewal charges keep their own data on the
--   payment row, so an unfinished card check never touches a working
--   subscription and a renewal is settled exactly once.
ALTER TABLE platform.cemetery_subscriptions ADD COLUMN paid_at TIMESTAMPTZ;
UPDATE platform.cemetery_subscriptions c SET paid_at = now()
 WHERE EXISTS (SELECT 1 FROM platform.invoices i WHERE i.tenant_id = c.tenant_id AND i.kind = 'invoice');

ALTER TABLE platform.card_payments
    ADD COLUMN plan          TEXT,
    ADD COLUMN recurrence_id TEXT,
    ADD COLUMN period_start  DATE,
    ADD COLUMN period_end    DATE,
    ADD COLUMN settled_at    TIMESTAMPTZ;

-- the sample cemetery is a demonstration for 30 days
UPDATE platform.cemetery_subscriptions SET trial_ends_at = now() + interval '30 days' WHERE status = 'demo' AND trial_ends_at IS NULL;
