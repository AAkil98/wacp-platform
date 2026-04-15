-- Login attempts: rate-limit tracking, GC'd after 24h.
-- Spec: wcon-data-model §5.7

CREATE TABLE login_attempts (
    id          TEXT    NOT NULL PRIMARY KEY,
    ip          TEXT    NOT NULL,
    username    TEXT    NOT NULL,
    attempted_at TEXT   NOT NULL,
    success     INTEGER NOT NULL,

    CHECK (success IN (0, 1))
);

CREATE INDEX idx_login_attempts_ip ON login_attempts (ip, attempted_at);
CREATE INDEX idx_login_attempts_username ON login_attempts (username, attempted_at);
