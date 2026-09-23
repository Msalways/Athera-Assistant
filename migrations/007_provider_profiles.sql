CREATE TABLE provider_profiles (
    provider_id TEXT PRIMARY KEY,
    auth_option_id TEXT NOT NULL,
    non_secret_config TEXT NOT NULL DEFAULT '{}',
    schema_version INTEGER NOT NULL DEFAULT 1,
    enabled INTEGER NOT NULL DEFAULT 1,
    display_name TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE credential_metadata (
    credential_handle TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    purpose TEXT NOT NULL,
    provider_id TEXT,
    connection_id TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    last_verified_at INTEGER,
    status TEXT NOT NULL DEFAULT 'active',
    FOREIGN KEY (provider_id) REFERENCES provider_profiles(provider_id)
) WITHOUT ROWID;

CREATE INDEX idx_credential_metadata_owner ON credential_metadata(owner_id);
CREATE INDEX idx_credential_metadata_provider ON credential_metadata(provider_id);
CREATE INDEX idx_credential_metadata_status ON credential_metadata(status);

INSERT OR IGNORE INTO schema_migrations VALUES(7);
