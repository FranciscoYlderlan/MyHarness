-- Initial migration: records which app version created the database.
CREATE TABLE schema_info (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    app_version TEXT NOT NULL
);

INSERT INTO schema_info (id, app_version) VALUES (1, '0.1.0');
