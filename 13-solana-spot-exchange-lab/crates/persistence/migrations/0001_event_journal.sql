CREATE TABLE IF NOT EXISTS event_journal (
    event_id BIGSERIAL PRIMARY KEY,
    command_id NUMERIC(39, 0) NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
