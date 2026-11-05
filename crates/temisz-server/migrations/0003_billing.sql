-- W3: subscriptions, billing details, invoices (TEMISZ's own invoices to customers).

CREATE TABLE platform.billing_details (
    tenant_id   TEXT PRIMARY KEY REFERENCES platform.tenants(id),
    name        TEXT NOT NULL,              -- legal name on the invoice
    tax_number  TEXT NOT NULL DEFAULT '',
    postcode    TEXT NOT NULL,
    city        TEXT NOT NULL,
    address     TEXT NOT NULL,
    email       TEXT NOT NULL,              -- invoices are sent here
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.subscriptions (
    tenant_id              TEXT PRIMARY KEY REFERENCES platform.tenants(id),
    plan                   TEXT NOT NULL CHECK (plan IN ('monthly', 'yearly')),
    payment_method         TEXT NOT NULL CHECK (payment_method IN ('card', 'transfer')),
    cemetery_count         BIGINT NOT NULL,          -- billed quantity
    status                 TEXT NOT NULL CHECK (status IN ('pending', 'active', 'past_due', 'canceled')),
    current_period_start   DATE,
    current_period_end     DATE,                     -- exclusive
    cancel_at_period_end   BOOLEAN NOT NULL DEFAULT false,
    stripe_customer_id     TEXT,
    stripe_subscription_id TEXT,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.invoices (
    id            TEXT PRIMARY KEY,
    tenant_id     TEXT NOT NULL REFERENCES platform.tenants(id),
    kind          TEXT NOT NULL CHECK (kind IN ('proforma', 'invoice')),
    number        TEXT NOT NULL,                 -- Számlázz.hu number (or TESZT-… without a key)
    proforma_id   TEXT REFERENCES platform.invoices(id),  -- invoice issued for a paid proforma
    description   TEXT NOT NULL,
    quantity      BIGINT NOT NULL,
    unit_net_huf  BIGINT NOT NULL,
    net_huf       BIGINT NOT NULL,
    vat_huf       BIGINT NOT NULL,
    gross_huf     BIGINT NOT NULL,
    period_start  DATE NOT NULL,
    period_end    DATE NOT NULL,
    due_date      DATE,
    payment_method TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('open', 'paid', 'void')),
    pdf           BYTEA,
    provider_log  TEXT NOT NULL DEFAULT '',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    paid_at       TIMESTAMPTZ
);
CREATE INDEX ix_invoices_tenant ON platform.invoices (tenant_id, created_at);

-- reminders already sent (trial day 23/28, deletion notices), so cron runs are idempotent
CREATE TABLE platform.notifications (
    tenant_id TEXT NOT NULL REFERENCES platform.tenants(id),
    key       TEXT NOT NULL,
    sent_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, key)
);
