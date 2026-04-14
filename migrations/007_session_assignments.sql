-- Session assignments: profile-to-role-slot bindings per session.
-- Spec: wcon-data-model §4.2

CREATE TABLE session_assignments (
    id          TEXT    NOT NULL PRIMARY KEY,
    session_id  TEXT    NOT NULL,

    role_ref        TEXT    NOT NULL,
    stage_id        TEXT,
    slot_position   INTEGER NOT NULL,
    profile_id      TEXT    NOT NULL,
    profile_version INTEGER NOT NULL,

    workspace_id TEXT,

    budget_max_cost_micros  INTEGER,
    budget_max_tokens       INTEGER,
    budget_max_wall_time_ms INTEGER,

    FOREIGN KEY (session_id) REFERENCES sessions (id) ON DELETE CASCADE,
    FOREIGN KEY (profile_id, profile_version) REFERENCES profiles (id, version)
);

CREATE INDEX idx_assignments_session ON session_assignments (session_id);
CREATE UNIQUE INDEX idx_assignments_slot ON session_assignments (session_id, slot_position);
