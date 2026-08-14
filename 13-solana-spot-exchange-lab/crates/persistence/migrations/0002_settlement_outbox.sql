CREATE TABLE IF NOT EXISTS settlement_outbox (
    outbox_id BIGSERIAL PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'submitted', 'failed')),
    request_payload JSONB NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    transaction_signature BYTEA,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS settlement_outbox_pending_idx
    ON settlement_outbox (outbox_id)
    WHERE status = 'pending';
