-- The last run of each scheduled job (the daily run), shown to the platform
-- admins: when it ran, what it did, and what failed.
CREATE TABLE IF NOT EXISTS platform.job_runs (
    job     TEXT PRIMARY KEY,
    ran_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    ok      BOOLEAN NOT NULL,
    result  TEXT NOT NULL DEFAULT '{}',
    errors  TEXT NOT NULL DEFAULT ''
);
