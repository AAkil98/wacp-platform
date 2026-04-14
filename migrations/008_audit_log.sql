-- Audit log: append-only mutation record.
-- Spec: wcon-data-model §5.6

CREATE TABLE audit_log (
    id          TEXT    NOT NULL PRIMARY KEY,
    user_id     TEXT    NOT NULL,
    timestamp   TEXT    NOT NULL,
    action      TEXT    NOT NULL,
    target_kind TEXT    NOT NULL,
    target_id   TEXT    NOT NULL,
    detail      TEXT,
    ip          TEXT    NOT NULL,
    user_agent  TEXT    NOT NULL,

    FOREIGN KEY (user_id) REFERENCES users (id)
);

CREATE INDEX idx_audit_log_timestamp ON audit_log (timestamp);
CREATE INDEX idx_audit_log_user ON audit_log (user_id);
CREATE INDEX idx_audit_log_action ON audit_log (action);
CREATE INDEX idx_audit_log_target ON audit_log (target_kind, target_id);
