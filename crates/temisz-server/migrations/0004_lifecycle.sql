-- W4: when an account became read-only or was closed; deletion bookkeeping.
ALTER TABLE platform.tenants
    ADD COLUMN read_only_since TIMESTAMPTZ,
    ADD COLUMN closed_at       TIMESTAMPTZ,
    ADD COLUMN deleted_at      TIMESTAMPTZ;
