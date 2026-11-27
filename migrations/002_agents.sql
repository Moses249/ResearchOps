CREATE TABLE IF NOT EXISTS agent_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    run_id TEXT NOT NULL REFERENCES runs(id),
    role TEXT NOT NULL,
    round INTEGER NOT NULL,
    status TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS agent_events_run ON agent_events(run_id,id);
CREATE TABLE IF NOT EXISTS agent_answers (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    answer TEXT NOT NULL
);
PRAGMA user_version=2;
