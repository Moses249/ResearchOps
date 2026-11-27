use researchops::engine::{Engine, ResearchSpec, SourceInput};
use tempfile::TempDir;

fn setup() -> (TempDir, Engine, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(dir.path().join("test.db")).unwrap();
    let id = engine
        .create(serde_json::from_str(include_str!("../examples/research.json")).unwrap())
        .unwrap();
    (dir, engine, id)
}

#[test]
fn end_to_end_persists_provenance_and_reports_a_real_gap() {
    let (dir, mut db, id) = setup();
    let result = db.work(&id, 100, 60).unwrap();
    assert_eq!(result.processed, 3);
    assert_eq!(result.run.status, "partial");
    assert_eq!(result.run.covered_questions, 2);
    assert_eq!(result.run.processed_questions, 3);
    assert!(result.run.tasks[2].findings.is_empty());
    for f in result.run.tasks.iter().flat_map(|t| &t.findings) {
        assert_eq!(f.verification, "exact_quote");
        assert_eq!(f.source_hash.len(), 64);
    }
    drop(db);
    let mut reopened = Engine::open(dir.path().join("test.db")).unwrap();
    assert_eq!(reopened.get(&id).unwrap().covered_questions, 2);
    assert_eq!(reopened.work(&id, 100, 60).unwrap().processed, 0);
    let report = reopened.report(&id).unwrap();
    assert!(report.contains("**Gap:**"));
    assert!(report.contains("not factual truth"));
    assert!(report.contains("SHA-256"));
}

#[test]
fn expired_lease_is_recovered_and_stale_worker_cannot_commit() {
    let (_dir, mut db, id) = setup();
    let old = db.claim(&id, 100, 10).unwrap().unwrap();
    let recovered = db.claim(&id, 111, 10).unwrap().unwrap();
    assert_eq!(old.task_id, recovered.task_id);
    assert_ne!(old.token, recovered.token);
    let findings = db.extract(&old).unwrap();
    assert!(db.finish(&old, &findings, 112).is_err());
    assert!(db.fail(&old, "late error", 112).is_err());
    db.finish(&recovered, &findings, 112).unwrap();
    assert!(db.finish(&recovered, &findings, 113).is_err());
    assert_eq!(db.get(&id).unwrap().tasks[0].attempts, 2);
}

#[test]
fn lease_expiration_rejects_results_even_before_reassignment() {
    let (_dir, mut db, id) = setup();
    let claim = db.claim(&id, 100, 10).unwrap().unwrap();
    assert!(db.finish(&claim, &[], 110).is_err());
}

#[test]
fn independent_connections_claim_different_tasks() {
    let (dir, mut first, id) = setup();
    let mut second = Engine::open(dir.path().join("test.db")).unwrap();
    let a = first.claim(&id, 100, 60).unwrap().unwrap();
    let b = second.claim(&id, 100, 60).unwrap().unwrap();
    assert_ne!(a.task_id, b.task_id);
}

#[test]
fn concurrent_workers_never_claim_the_same_unexpired_task() {
    let (dir, db, id) = setup();
    drop(db);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let mut handles = vec![];
    for _ in 0..4 {
        let path = dir.path().join("test.db");
        let run = id.clone();
        let gate = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut engine = Engine::open(path).unwrap();
            gate.wait();
            engine.claim(&run, 100, 60).unwrap().map(|c| c.task_id)
        }));
    }
    let tasks = handles
        .into_iter()
        .filter_map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 3);
    assert_eq!(
        tasks
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
}

#[test]
fn retry_backoff_and_terminal_failure_are_bounded() {
    let (_dir, mut db, id) = setup();
    let first = db.claim(&id, 100, 60).unwrap().unwrap();
    db.fail(&first, "temporary error", 101).unwrap();
    let second = db.claim(&id, 103, 60).unwrap().unwrap();
    assert_eq!(first.task_id, second.task_id);
    db.fail(&second, "temporary error", 104).unwrap();
    let third = db.claim(&id, 108, 60).unwrap().unwrap();
    db.fail(&third, "permanent error", 109).unwrap();
    let task = &db.get(&id).unwrap().tasks[0];
    assert_eq!(task.status, "failed");
    assert_eq!(task.attempts, 3);
    assert_eq!(task.last_error.as_deref(), Some("permanent error"));
}

#[test]
fn crashes_exhaust_attempts_without_silently_completing() {
    let (_dir, mut db, id) = setup();
    let a = db.claim(&id, 100, 1).unwrap().unwrap();
    let b = db.claim(&id, 101, 1).unwrap().unwrap();
    let c = db.claim(&id, 102, 1).unwrap().unwrap();
    assert_eq!(a.task_id, b.task_id);
    assert_eq!(b.task_id, c.task_id);
    db.claim(&id, 103, 1).unwrap();
    assert_eq!(db.get(&id).unwrap().tasks[0].status, "failed");
    assert_ne!(db.get(&id).unwrap().status, "complete");
}

#[test]
fn tampered_evidence_rolls_back_task_completion() {
    let (_dir, mut db, id) = setup();
    let claim = db.claim(&id, 100, 60).unwrap().unwrap();
    let mut findings = db.extract(&claim).unwrap();
    assert!(!findings.is_empty());
    findings[0].quote = "invented claim absent from source".into();
    assert!(db.finish(&claim, &findings, 101).is_err());
    let run = db.get(&id).unwrap();
    assert_eq!(run.tasks[0].status, "running");
    assert!(run.tasks[0].findings.is_empty());
    let valid = db.extract(&claim).unwrap();
    db.finish(&claim, &valid, 102).unwrap();
}

#[test]
fn duplicate_questions_and_snapshots_do_not_duplicate_work() {
    let (_dir, mut db, _id) = setup();
    let source = SourceInput {
        title: "A".into(),
        uri: "fixture://a".into(),
        content: "Leases support recovery.".into(),
    };
    let id = db
        .create(ResearchSpec {
            objective: "Recovery".into(),
            questions: vec!["Recovery".into(), " Recovery ".into()],
            sources: vec![source.clone(), source],
        })
        .unwrap();
    let result = db.work(&id, 100, 60).unwrap();
    assert_eq!(result.run.total_questions, 1);
    assert_eq!(result.run.tasks[0].findings.len(), 1);
    assert_eq!(result.run.status, "complete");
}

#[test]
fn invalid_specs_do_not_create_runs() {
    let (_dir, mut db, _id) = setup();
    let before = db.list().unwrap().len();
    let mut spec: ResearchSpec =
        serde_json::from_str(include_str!("../examples/research.json")).unwrap();
    spec.questions = vec!["the and is".into()];
    assert!(db.create(spec).is_err());
    assert_eq!(db.list().unwrap().len(), before);
}

#[test]
fn korean_unicode_and_untrusted_markup_remain_plain_evidence() {
    let (_dir, mut db, _) = setup();
    let id = db
        .create(ResearchSpec {
            objective: "<script>alert(1)</script>".into(),
            questions: vec!["작업 복구".into()],
            sources: vec![SourceInput {
                title: "<img src=x>".into(),
                uri: "javascript:alert(1)".into(),
                content: "작업 복구 기능을 확인합니다. <script>untrusted()</script>".into(),
            }],
        })
        .unwrap();
    let run = db.work(&id, 100, 60).unwrap().run;
    assert_eq!(run.covered_questions, 1);
    let report = db.report(&id).unwrap();
    assert!(!report.contains("<script>"));
    assert!(!report.contains("<img"));
}
