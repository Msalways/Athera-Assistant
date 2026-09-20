ALTER TABLE run_events ADD COLUMN worker_id TEXT;
ALTER TABLE run_events ADD COLUMN text_delta TEXT;
INSERT OR IGNORE INTO schema_migrations VALUES(4);
