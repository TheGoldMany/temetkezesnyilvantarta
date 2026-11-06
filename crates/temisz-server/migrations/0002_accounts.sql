-- W2: e-mail verification, password reset, invitations, two-factor sign-in,
-- outgoing e-mail log and simple rate limiting.

ALTER TABLE platform.accounts
    ADD COLUMN totp_secret     TEXT,
    ADD COLUMN totp_enabled_at TIMESTAMPTZ,
    ADD COLUMN recovery_codes  TEXT NOT NULL DEFAULT '[]';   -- JSON array of SHA-256 hashes

CREATE TABLE platform.tokens (
    token_hash TEXT PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('verify', 'reset', 'invite', 'mfa')),
    account_id TEXT NOT NULL REFERENCES platform.accounts(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ
);
CREATE INDEX ix_tokens_account ON platform.tokens (account_id, kind);

CREATE TABLE platform.emails (
    id         TEXT PRIMARY KEY,
    to_address TEXT NOT NULL,
    subject    TEXT NOT NULL,
    body       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at    TIMESTAMPTZ,
    error      TEXT
);
CREATE INDEX ix_emails_to ON platform.emails (lower(to_address), created_at);

CREATE TABLE platform.rate_events (
    key TEXT NOT NULL,
    ts  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_rate_events ON platform.rate_events (key, ts);

-- Set at sign-in for admins who must turn on two-factor sign-in; until then
-- the session can only do that (colleagues invited mid-session do not cut in).
ALTER TABLE platform.sessions ADD COLUMN mfa_setup_required BOOLEAN NOT NULL DEFAULT false;
