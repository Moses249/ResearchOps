use researchops::engine::Engine;

#[test]
fn version_one_and_two_databases_are_preserved_without_invented_proof() {
    for version in [1, 2] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        if version == 2 {
            conn.execute_batch(include_str!("../migrations/002_agents.sql"))
                .unwrap();
        }
        conn.execute_batch("INSERT INTO runs VALUES('legacy','Historical research',1); INSERT INTO tasks(id,run_id,question,status,attempts) VALUES('task','legacy','What is recovery?','succeeded',1);").unwrap();
        drop(conn);
        let engine = Engine::open(&path).unwrap();
        let run = engine.get("legacy").unwrap();
        assert_eq!(run.tasks[0].status, "succeeded");
        assert_eq!(run.execution.counts.readback_verified, 0);
        assert_ne!(run.execution.business_status, "COMPLETE");
        assert!(run
            .execution
            .gate_failures
            .iter()
            .any(|g| g.contains("contract")));
        drop(engine);
        Engine::open(&path).unwrap();
        let conn = rusqlite::Connection::open(path).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM batch_readbacks", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn append_only_ledgers_and_source_snapshots_reject_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.db");
    let mut engine = Engine::open(&path).unwrap();
    let id = engine
        .create(serde_json::from_str(include_str!("../examples/research.json")).unwrap())
        .unwrap();
    engine.work(&id, 100, 60).unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    for statement in [
        "UPDATE sources SET content='changed'",
        "DELETE FROM sources",
        "UPDATE findings SET score=score+1",
        "DELETE FROM findings",
        "UPDATE execution_batches SET payload='changed'",
        "DELETE FROM execution_batches",
        "UPDATE batch_readbacks SET sha256='changed'",
        "DELETE FROM batch_readbacks",
        "DELETE FROM execution_contracts",
        "INSERT OR REPLACE INTO sources SELECT id,run_id,title,uri,'changed',content_hash FROM sources",
        "INSERT OR REPLACE INTO execution_contracts SELECT version,'{}',sha256,md5,created_at FROM execution_contracts",
        "INSERT OR REPLACE INTO findings SELECT task_id,source_id,quote,score+1,verification FROM findings",
        "INSERT OR REPLACE INTO execution_batches SELECT id,run_id,task_id,lease_token,'changed',sha256,md5,findings_count,created_at FROM execution_batches",
        "INSERT OR REPLACE INTO batch_readbacks SELECT batch_id,'changed',findings_count,task_status,checked_at FROM batch_readbacks",
        "INSERT OR REPLACE INTO run_contracts SELECT run_id,version,'changed',expected_md5,mode FROM run_contracts",
    ] {
        assert!(
            conn.execute(statement, []).is_err(),
            "accepted mutation: {statement}"
        );
    }
}

#[test]
fn version_three_upgrade_preserves_observations_and_adds_collision_guards() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v3.db");
    let mut engine = Engine::open(&path).unwrap();
    let id = engine
        .create(serde_json::from_str(include_str!("../examples/research.json")).unwrap())
        .unwrap();
    engine.work(&id, 100, 60).unwrap();
    drop(engine);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let guards = conn.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND (name LIKE '%_no_replace' OR name IN ('findings_no_UPDATE','findings_no_DELETE','run_contract_scope_fixed'))").unwrap()
        .query_map([], |r| r.get::<_, String>(0)).unwrap()
        .collect::<rusqlite::Result<Vec<_>>>().unwrap();
    for guard in guards {
        conn.execute_batch(&format!("DROP TRIGGER {guard}"))
            .unwrap();
    }
    conn.execute_batch("PRAGMA user_version=3").unwrap();
    let before: i64 = conn
        .query_row("SELECT count(*) FROM findings", [], |r| r.get(0))
        .unwrap();
    drop(conn);
    let engine = Engine::open(&path).unwrap();
    assert_eq!(
        engine.get(&id).unwrap().execution.counts.readback_verified,
        3
    );
    drop(engine);
    let conn = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM findings", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        before
    );
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert!(conn.execute("UPDATE findings SET score=0", []).is_err());
    assert!(conn.execute("INSERT OR REPLACE INTO sources SELECT id,run_id,title,uri,'changed',content_hash FROM sources", []).is_err());
}
