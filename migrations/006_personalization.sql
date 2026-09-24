-- v5 stores milliseconds from the UI runtime and seconds from model proposals.
UPDATE adaptive_rules SET data=json_set(data,
 '$.created_at', CASE WHEN json_extract(data,'$.created_at') BETWEEN 1000000000 AND 9999999999 THEN json_extract(data,'$.created_at')*1000 ELSE json_extract(data,'$.created_at') END,
 '$.updated_at', CASE WHEN json_extract(data,'$.updated_at') BETWEEN 1000000000 AND 9999999999 THEN json_extract(data,'$.updated_at')*1000 ELSE json_extract(data,'$.updated_at') END,
 '$.expires_at', CASE WHEN json_extract(data,'$.expires_at') BETWEEN 1000000000 AND 9999999999 THEN json_extract(data,'$.expires_at')*1000 ELSE json_extract(data,'$.expires_at') END);
CREATE TABLE personal_revisions(
 rule_id TEXT NOT NULL, version INTEGER NOT NULL, data TEXT NOT NULL CHECK(json_valid(data)),
 decision TEXT NOT NULL, baseline INTEGER NOT NULL DEFAULT 0,
 PRIMARY KEY(rule_id,version)
);
INSERT INTO personal_revisions SELECT id,json_extract(data,'$.version'),data,'baseline',1 FROM adaptive_rules;
UPDATE rule_proposals SET status=(SELECT status FROM adaptive_rules WHERE id=json_extract(rule_proposals.data,'$.rule.id')),
 data=json_set(data,'$.rule',json((SELECT data FROM adaptive_rules WHERE id=json_extract(rule_proposals.data,'$.rule.id'))))
 WHERE EXISTS(SELECT 1 FROM adaptive_rules WHERE id=json_extract(rule_proposals.data,'$.rule.id'));
CREATE TABLE personal_observations(
 id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, source_id TEXT NOT NULL,
 kind TEXT NOT NULL, data TEXT NOT NULL CHECK(json_valid(data)), processed INTEGER NOT NULL DEFAULT 0,
 UNIQUE(source_id,kind,data)
);
CREATE TABLE personal_usage(
 run_id TEXT NOT NULL, conversation_id TEXT NOT NULL, rule_id TEXT NOT NULL, version INTEGER NOT NULL,
 PRIMARY KEY(run_id,rule_id,version)
);
CREATE TABLE generated_skills(rule_id TEXT PRIMARY KEY, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE skill_evaluations(id INTEGER PRIMARY KEY,rule_id TEXT NOT NULL,version INTEGER NOT NULL,data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE learning_candidates(observation_id TEXT PRIMARY KEY, proposal_id TEXT NOT NULL);
CREATE TABLE personal_replacements(rule_id TEXT PRIMARY KEY,previous_id TEXT NOT NULL,previous_version INTEGER NOT NULL);
INSERT OR IGNORE INTO schema_migrations VALUES(6);
