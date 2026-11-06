-- Funeral offices invited into the account share the funeral calendar.
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check;
ALTER TABLE users ADD CONSTRAINT users_role_check CHECK (role IN ('admin', 'clerk', 'finance', 'reader', 'cashier', 'worker', 'undertaker'));
