ALTER TABLE agent_events ADD COLUMN lease_token TEXT;
CREATE TABLE execution_contracts (
 version TEXT PRIMARY KEY, body TEXT NOT NULL, sha256 TEXT NOT NULL, md5 TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE run_contracts (
 run_id TEXT PRIMARY KEY REFERENCES runs(id), version TEXT NOT NULL REFERENCES execution_contracts(version), expected_sha256 TEXT NOT NULL, expected_md5 TEXT NOT NULL, mode TEXT CHECK(mode IN ('local','agents'))
);
CREATE TABLE execution_batches (
 id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id), lease_token TEXT NOT NULL,
 payload TEXT NOT NULL, sha256 TEXT NOT NULL, md5 TEXT NOT NULL, findings_count INTEGER NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE batch_readbacks (
 batch_id TEXT PRIMARY KEY REFERENCES execution_batches(id), sha256 TEXT NOT NULL, findings_count INTEGER NOT NULL, task_status TEXT NOT NULL, checked_at INTEGER NOT NULL
);
CREATE TABLE model_review_receipts (
 task_id TEXT PRIMARY KEY REFERENCES tasks(id), lease_token TEXT NOT NULL, answer_sha256 TEXT NOT NULL, reviewed_at INTEGER NOT NULL
);
CREATE TABLE report_artifacts (
 id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), body TEXT NOT NULL, sha256 TEXT NOT NULL, input_sha256 TEXT NOT NULL, created_at INTEGER NOT NULL,
 UNIQUE(run_id,input_sha256,sha256)
);
CREATE TABLE artifact_readbacks (
 artifact_id TEXT PRIMARY KEY REFERENCES report_artifacts(id), sha256 TEXT NOT NULL, input_sha256 TEXT NOT NULL, checked_at INTEGER NOT NULL
);
CREATE TABLE execution_sessions (
 id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), mode TEXT NOT NULL CHECK(mode IN ('local','agents')),
 trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('manual','scheduled','recovery')),
 system_status TEXT NOT NULL CHECK(system_status IN ('running','completed','failed','interrupted')), business_status TEXT NOT NULL,
 started_at INTEGER NOT NULL, finished_at INTEGER, start_kst_date TEXT NOT NULL, end_kst_date TEXT, elapsed_ms INTEGER NOT NULL DEFAULT 0,
 lease_until INTEGER NOT NULL, budget_seconds INTEGER NOT NULL, before_counts TEXT NOT NULL, after_counts TEXT,
 stop_code TEXT, last_successful_stage TEXT, failure_stage TEXT, error TEXT, resume_json TEXT, final_input_sha256 TEXT
);
CREATE UNIQUE INDEX execution_live_run ON execution_sessions(run_id) WHERE system_status='running';
CREATE INDEX execution_run ON execution_sessions(run_id,started_at);
CREATE TRIGGER run_contract_binding_frozen BEFORE UPDATE ON run_contracts
WHEN (OLD.mode IS NOT NULL OR EXISTS(SELECT 1 FROM tasks WHERE run_id=OLD.run_id AND attempts>0) OR EXISTS(SELECT 1 FROM execution_sessions WHERE run_id=OLD.run_id))
AND NOT(OLD.mode IS NULL AND NEW.mode IN ('local','agents') AND NEW.version=OLD.version AND NEW.expected_sha256=OLD.expected_sha256 AND NEW.expected_md5=OLD.expected_md5)
BEGIN SELECT RAISE(ABORT,'run contract and mode are pinned'); END;
CREATE TRIGGER run_contract_no_delete BEFORE DELETE ON run_contracts
BEGIN SELECT RAISE(ABORT,'run contract binding cannot be deleted'); END;
PRAGMA user_version=3;
