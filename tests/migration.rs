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
            3
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
        "UPDATE execution_batches SET payload='changed'",
        "DELETE FROM execution_batches",
        "UPDATE batch_readbacks SET sha256='changed'",
        "DELETE FROM batch_readbacks",
        "DELETE FROM execution_contracts",
    ] {
        assert!(
            conn.execute(statement, []).is_err(),
            "accepted mutation: {statement}"
        );
    }
}
