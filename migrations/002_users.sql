-- Users: local identity store with Argon2id password hashing.
-- Spec: wcon-data-model §5.3

CREATE TABLE users (
    id                   TEXT    NOT NULL PRIMARY KEY,
    username             TEXT    NOT NULL,
    username_lower       TEXT    NOT NULL UNIQUE,
    display_name         TEXT    NOT NULL,
    password_hash        TEXT    NOT NULL,
    console_role         TEXT    NOT NULL DEFAULT 'operator',
    must_change_password INTEGER NOT NULL DEFAULT 1,
    disabled_at          TEXT,
    created_at           TEXT    NOT NULL,
    updated_at           TEXT    NOT NULL,

    CHECK (console_role IN ('admin', 'operator', 'viewer'))
);
