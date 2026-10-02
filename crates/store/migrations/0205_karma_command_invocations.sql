CREATE TABLE karma_command_invocation (
    uid TEXT PRIMARY KEY,
    command_uid TEXT,
    command_revision INTEGER,
    configuration TEXT NOT NULL,
    host_uid TEXT,
    actor_uid TEXT,
    numeric INTEGER NOT NULL CHECK (numeric IN (0, 1)),
    context TEXT,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed', 'indeterminate')),
    stdout TEXT,
    stderr TEXT,
    value TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT
);
CREATE INDEX karma_command_invocation_command ON karma_command_invocation(command_uid, created_at);
CREATE TABLE karma_signal_sample (
    signal_uid TEXT PRIMARY KEY REFERENCES record(uid),
    invocation_uid TEXT NOT NULL REFERENCES karma_command_invocation(uid),
    value TEXT NOT NULL,
    sampled_at TEXT NOT NULL
);
