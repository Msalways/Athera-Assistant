-- Diagnostics log: internal actions and the exact error behind them.
--
-- Written for the person debugging, not the person using. It records what the
-- runtime actually attempted and what came back, including the provider's own
-- wording, so a fault can be traced without a debugger attached. Kept apart from
-- run_events, which is the user-facing story of a task, because this records the
-- parts that never surface: the retries, the attempts that were abandoned, and
-- the raw reason each one failed.
--
-- `detail` is bounded so an unexpected provider payload cannot fill the device.
-- Nothing here is a secret: `detail` is redacted at the call site before it is
-- written, and no credential, header or token may be passed into it.
CREATE TABLE IF NOT EXISTS diagnostics (
    sequence   INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    task_id    TEXT,
    category   TEXT    NOT NULL,
    subject    TEXT    NOT NULL,
    outcome    TEXT    NOT NULL,
    detail     TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS diagnostics_created_at ON diagnostics(created_at);
CREATE INDEX IF NOT EXISTS diagnostics_task_id ON diagnostics(task_id);
CREATE INDEX IF NOT EXISTS diagnostics_category ON diagnostics(category);
