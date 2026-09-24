CREATE TABLE IF NOT EXISTS adaptive_rules(
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    scope TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    data TEXT NOT NULL CHECK(json_valid(data))
);
CREATE TABLE IF NOT EXISTS rule_proposals(
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    proposed_at INTEGER NOT NULL,
    data TEXT NOT NULL CHECK(json_valid(data))
);
INSERT OR IGNORE INTO schema_migrations VALUES(5);
