CREATE TABLE IF NOT EXISTS run_events(
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    status TEXT NOT NULL,
    step INTEGER NOT NULL,
    kind TEXT,
    message TEXT NOT NULL,
    output TEXT CHECK(output IS NULL OR json_valid(output))
);

INSERT OR IGNORE INTO run_events(sequence, task_id, status, step, kind, message, output)
SELECT sequence, task_id, status, step, NULL, '', NULL FROM events;

INSERT OR IGNORE INTO schema_migrations VALUES(3);
