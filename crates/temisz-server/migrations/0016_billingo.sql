-- TEMISZ's own invoices in Billingo: the customer's partner id there.
ALTER TABLE platform.billing_details ADD COLUMN IF NOT EXISTS billingo_partner_id BIGINT;
