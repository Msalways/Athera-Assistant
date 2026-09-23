CREATE TABLE routing_provenance(
  task_id TEXT PRIMARY KEY,
  strategy TEXT NOT NULL,
  provider_id TEXT,
  reason TEXT NOT NULL DEFAULT '',
  egress TEXT NOT NULL DEFAULT 'none',
  latency_ms INTEGER,
  routed_at INTEGER NOT NULL,
  data TEXT NOT NULL CHECK(json_valid(data))
) WITHOUT ROWID;
CREATE TABLE model_usage(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT NOT NULL,
  prompt_tokens INTEGER NOT NULL,
  completion_tokens INTEGER NOT NULL,
  total_tokens INTEGER NOT NULL,
  model_id TEXT NOT NULL DEFAULT '',
  recorded_at INTEGER NOT NULL
);
CREATE INDEX idx_model_usage_task ON model_usage(task_id);
INSERT OR IGNORE INTO schema_migrations VALUES(8);
