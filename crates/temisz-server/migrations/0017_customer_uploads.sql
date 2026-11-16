-- A registered customer can send its register from the program (Első lépések):
-- the upload is kept with the demo requests, marked with the organisation.
ALTER TABLE platform.demo_requests ADD COLUMN IF NOT EXISTS tenant_id TEXT;
