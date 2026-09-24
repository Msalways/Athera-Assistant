CREATE TABLE jobs(
  id TEXT PRIMARY KEY,
  task_id TEXT,
  objective TEXT NOT NULL,
  success_condition TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  run_at INTEGER,
  condition TEXT,
  allowed_tools TEXT NOT NULL DEFAULT '[]',
  max_actions INTEGER NOT NULL DEFAULT 10,
  actions_taken INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  data TEXT NOT NULL CHECK(json_valid(data))
) WITHOUT ROWID;
CREATE INDEX idx_jobs_status_run_at ON jobs(status, run_at);
INSERT OR IGNORE INTO schema_migrations VALUES(10);
