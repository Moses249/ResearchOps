-- REPLACE can bypass DELETE triggers when recursive_triggers is off.
-- Reject collisions before conflict handling; idempotent application inserts
-- explicitly SELECT only missing identities and still verify stored contents.
CREATE TRIGGER run_contract_scope_fixed BEFORE UPDATE OF run_id ON run_contracts WHEN NEW.run_id!=OLD.run_id BEGIN SELECT RAISE(ABORT,'run contract scope is fixed'); END;
CREATE TRIGGER findings_no_UPDATE BEFORE UPDATE ON findings BEGIN SELECT RAISE(ABORT,'append-only ledger'); END;
CREATE TRIGGER findings_no_DELETE BEFORE DELETE ON findings BEGIN SELECT RAISE(ABORT,'append-only ledger'); END;
CREATE TRIGGER sources_no_replace BEFORE INSERT ON sources WHEN EXISTS(SELECT 1 FROM sources WHERE id=NEW.id OR (run_id=NEW.run_id AND content_hash=NEW.content_hash)) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER findings_no_replace BEFORE INSERT ON findings WHEN EXISTS(SELECT 1 FROM findings WHERE task_id=NEW.task_id AND source_id=NEW.source_id AND quote=NEW.quote) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER agent_events_no_replace BEFORE INSERT ON agent_events WHEN EXISTS(SELECT 1 FROM agent_events WHERE id=NEW.id) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER agent_answers_no_replace BEFORE INSERT ON agent_answers WHEN EXISTS(SELECT 1 FROM agent_answers WHERE task_id=NEW.task_id) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER execution_contracts_no_replace BEFORE INSERT ON execution_contracts WHEN EXISTS(SELECT 1 FROM execution_contracts WHERE version=NEW.version) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER execution_batches_no_replace BEFORE INSERT ON execution_batches WHEN EXISTS(SELECT 1 FROM execution_batches WHERE id=NEW.id OR task_id=NEW.task_id) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER batch_readbacks_no_replace BEFORE INSERT ON batch_readbacks WHEN EXISTS(SELECT 1 FROM batch_readbacks WHERE batch_id=NEW.batch_id) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER model_review_receipts_no_replace BEFORE INSERT ON model_review_receipts WHEN EXISTS(SELECT 1 FROM model_review_receipts WHERE task_id=NEW.task_id) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER report_artifacts_no_replace BEFORE INSERT ON report_artifacts WHEN EXISTS(SELECT 1 FROM report_artifacts WHERE id=NEW.id OR (run_id=NEW.run_id AND input_sha256=NEW.input_sha256 AND sha256=NEW.sha256)) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER artifact_readbacks_no_replace BEFORE INSERT ON artifact_readbacks WHEN EXISTS(SELECT 1 FROM artifact_readbacks WHERE artifact_id=NEW.artifact_id) BEGIN SELECT RAISE(ABORT,'append-only ledger identity'); END;
CREATE TRIGGER run_contract_no_replace BEFORE INSERT ON run_contracts WHEN EXISTS(SELECT 1 FROM run_contracts WHERE run_id=NEW.run_id) AND (EXISTS(SELECT 1 FROM run_contracts WHERE run_id=NEW.run_id AND mode IS NOT NULL) OR EXISTS(SELECT 1 FROM tasks WHERE run_id=NEW.run_id AND attempts>0) OR EXISTS(SELECT 1 FROM execution_sessions WHERE run_id=NEW.run_id)) BEGIN SELECT RAISE(ABORT,'run contract and mode are pinned'); END;
PRAGMA user_version=4;
