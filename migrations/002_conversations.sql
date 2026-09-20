CREATE TABLE IF NOT EXISTS conversations(
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages(
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('generating', 'complete', 'cancelled', 'failed', 'interrupted')),
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS messages_conversation_sequence
    ON messages(conversation_id, sequence);

CREATE TABLE IF NOT EXISTS personal_memories(
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS research_sessions(
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    sources TEXT NOT NULL CHECK(json_valid(sources)),
    hypotheses TEXT NOT NULL CHECK(json_valid(hypotheses)),
    decisions TEXT NOT NULL CHECK(json_valid(decisions)),
    experiments TEXT NOT NULL CHECK(json_valid(experiments))
);
CREATE INDEX IF NOT EXISTS research_sessions_conversation
    ON research_sessions(conversation_id);

CREATE TABLE IF NOT EXISTS research_notes(
    session_id TEXT NOT NULL REFERENCES research_sessions(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision > 0),
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(session_id, revision)
);

INSERT OR IGNORE INTO schema_migrations VALUES(2);
