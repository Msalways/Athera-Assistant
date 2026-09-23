CREATE TABLE task_blockers(
  task_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  data TEXT NOT NULL CHECK(json_valid(data)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;
INSERT OR IGNORE INTO schema_migrations VALUES(9);
