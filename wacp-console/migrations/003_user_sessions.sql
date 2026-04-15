-- User sessions: cookie-based browser sessions.
-- Spec: wcon-data-model §5.4

CREATE TABLE user_sessions (
    id          TEXT    NOT NULL PRIMARY KEY,
    user_id     TEXT    NOT NULL,
    token_hash  TEXT    NOT NULL UNIQUE,
    ip          TEXT    NOT NULL,
    user_agent  TEXT    NOT NULL,
    created_at  TEXT    NOT NULL,
    expires_at  TEXT    NOT NULL,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_user_sessions_user ON user_sessions (user_id);
CREATE INDEX idx_user_sessions_expires ON user_sessions (expires_at);
