-- Profiles: agent configuration bundles with append-only versioning.
-- Spec: wcon-data-model §3

CREATE TABLE profiles (
    id          TEXT    NOT NULL,
    version     INTEGER NOT NULL,

    name        TEXT    NOT NULL,
    description TEXT,
    tags        TEXT,

    role_ref    TEXT    NOT NULL,

    llm_provider    TEXT    NOT NULL,
    llm_model       TEXT    NOT NULL,
    llm_temperature REAL,
    llm_max_tokens  INTEGER,

    autonomy    TEXT    NOT NULL DEFAULT 'assisted',

    tool_allowlist TEXT,
    tool_denylist  TEXT,

    budget_max_cost_micros    INTEGER,
    budget_max_tokens         INTEGER,
    budget_max_wall_time_ms   INTEGER,
    budget_warning_threshold  REAL DEFAULT 0.8,

    owner_user_id TEXT NOT NULL,
    visibility    TEXT NOT NULL DEFAULT 'private',

    is_current  INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL,
    deleted_at  TEXT,

    PRIMARY KEY (id, version),
    FOREIGN KEY (owner_user_id) REFERENCES users (id),
    CHECK (autonomy IN ('autonomous', 'assisted', 'supervised')),
    CHECK (visibility IN ('private', 'shared')),
    CHECK (budget_warning_threshold >= 0.0 AND budget_warning_threshold <= 1.0)
);

CREATE INDEX idx_profiles_current ON profiles (id) WHERE is_current = 1 AND deleted_at IS NULL;
CREATE INDEX idx_profiles_role ON profiles (role_ref) WHERE is_current = 1 AND deleted_at IS NULL;
CREATE INDEX idx_profiles_name ON profiles (name) WHERE is_current = 1 AND deleted_at IS NULL;
CREATE INDEX idx_profiles_owner ON profiles (owner_user_id) WHERE is_current = 1 AND deleted_at IS NULL;
