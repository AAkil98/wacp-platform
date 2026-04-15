-- Sessions: coordination runs with state machine lifecycle.
-- Spec: wcon-data-model §4

CREATE TABLE sessions (
    id          TEXT    NOT NULL PRIMARY KEY,

    name        TEXT,

    owner_user_id TEXT NOT NULL,

    vertical    TEXT    NOT NULL,
    workflow    TEXT    NOT NULL,
    context     TEXT,

    coordinator_workspace_id TEXT,

    state       TEXT    NOT NULL DEFAULT 'configuring',
    created_at  TEXT    NOT NULL,
    launched_at TEXT,
    closed_at   TEXT,

    budget_max_cost_micros  INTEGER,
    budget_max_tokens       INTEGER,
    budget_max_wall_time_ms INTEGER,

    FOREIGN KEY (owner_user_id) REFERENCES users (id),
    CHECK (state IN (
        'configuring',
        'validating',
        'launching',
        'active',
        'completed',
        'failed',
        'cancelled'
    ))
);

CREATE INDEX idx_sessions_state ON sessions (state);
CREATE INDEX idx_sessions_created ON sessions (created_at);
CREATE INDEX idx_sessions_owner ON sessions (owner_user_id);
