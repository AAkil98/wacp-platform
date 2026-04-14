-- Settings: key-value configuration store.
-- Spec: wcon-data-model §5.1

CREATE TABLE settings (
    key         TEXT NOT NULL PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
