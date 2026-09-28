-- Shadow observations of the on-device model.
-- Holds the local model's proposal and what the cloud model did, never the
-- user's own words: enough to measure whether the local model's confidence
-- transfers to this product's tools, without keeping a second copy of user text.
CREATE TABLE IF NOT EXISTS local_probes (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  data TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_local_probes_created ON local_probes(created_at);
CREATE INDEX IF NOT EXISTS idx_local_probes_task ON local_probes(task_id);
