CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    objective TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id),
    question TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','succeeded','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at INTEGER NOT NULL DEFAULT 0,
    lease_token TEXT,
    lease_until INTEGER,
    last_error TEXT,
    UNIQUE(run_id,question)
);
CREATE INDEX IF NOT EXISTS tasks_claim ON tasks(run_id,status,available_at,lease_until);
CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id),
    title TEXT NOT NULL,
    uri TEXT NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    UNIQUE(run_id,content_hash)
);
CREATE TABLE IF NOT EXISTS findings (
    task_id TEXT NOT NULL REFERENCES tasks(id),
    source_id TEXT NOT NULL REFERENCES sources(id),
    quote TEXT NOT NULL,
    score REAL NOT NULL,
    verification TEXT NOT NULL CHECK(verification='exact_quote'),
    PRIMARY KEY(task_id,source_id,quote)
);
PRAGMA user_version=1;
