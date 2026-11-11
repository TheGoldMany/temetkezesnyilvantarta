-- A colleague can be limited to some of the organisation's cemeteries
-- (JSON array of cemetery ids; empty = every cemetery). Administrators
-- always see every cemetery.
ALTER TABLE users ADD COLUMN cemetery_ids TEXT NOT NULL DEFAULT '[]';
