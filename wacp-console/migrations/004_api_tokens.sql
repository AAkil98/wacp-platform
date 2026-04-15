-- API tokens: bearer tokens for programmatic access.
-- Spec: wcon-data-model §5.5

CREATE TABLE api_tokens (
    id          TEXT    NOT NULL PRIMARY KEY,
    user_id     TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    token_hash  TEXT    NOT NULL UNIQUE,
    created_at  TEXT    NOT NULL,
    expires_at  TEXT,
    last_used_at TEXT,
    revoked_at  TEXT,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_api_tokens_user ON api_tokens (user_id);
