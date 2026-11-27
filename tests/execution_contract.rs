use researchops::{
    contracts::{md5_digest, Binding, ContractImport, Mode, Profile, Trigger},
    engine::{Engine, ResearchSpec, SourceInput},
    execution::{self, Clock, SystemClock},
};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

fn setup() -> (tempfile::TempDir, Engine, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(dir.path().join("test.db")).unwrap();
    let id = engine
        .create(ResearchSpec {
            objective: "Understand recovery".into(),
            questions: vec![
                "What is recovery?".into(),
                "What enables leases?".into(),
                "What is persistence?".into(),
            ],
            sources: vec![SourceInput {
                title: "Fixture".into(),
                uri: "fixture://core".into(),
                content:
                    "Recovery restores work. Leases enable ownership. Persistence saves state."
                        .into(),
            }],
        })
        .unwrap();
    (dir, engine, id)
}
fn bind(engine: &mut Engine, id: &str, profile: &str, budget: u64, batch: usize) {
    let body =
        json!({"profile":profile,"max_runtime_seconds":budget,"batch_size":batch,"sql_retries":2})
            .to_string();
    let version = format!("test-{profile}-{budget}-{batch}");
    let expected_md5 = md5_digest(&body);
    engine
        .register_contract(ContractImport {
            version: version.clone(),
            body,
            expected_md5: expected_md5.clone(),
        })
        .unwrap();
    engine
        .bind_contract(
            id,
            Binding {
                version,
                expected_md5,
            },
        )
        .unwrap();
}

#[test]
fn contracts_validate_digest_limits_and_pinned_mode_before_claim() {
    let (_dir, mut engine, id) = setup();
    let before = engine.contracts().unwrap().len();
    assert!(engine
        .register_contract(ContractImport {
            version: "bad".into(),
            body: "{}".into(),
            expected_md5: "bad".into()
        })
        .is_err());
    assert_eq!(engine.contracts().unwrap().len(), before);
    let body =
        r#"{"profile":"research","max_runtime_seconds":3601,"batch_size":1,"sql_retries":2}"#
            .to_string();
    assert!(engine
        .register_contract(ContractImport {
            version: "invalid-limit".into(),
            expected_md5: md5_digest(&body),
            body
        })
        .is_err());
    bind(&mut engine, &id, "model_reviewed_research", 10, 1);
    assert_eq!(
        engine.checked_policy(&id).unwrap().profile,
        Profile::ModelReviewedResearch
    );
    assert!(engine.work(&id, 100, 60).is_err());
    assert_eq!(engine.get(&id).unwrap().tasks[0].attempts, 0);
    engine.authorize_mode(&id, Mode::Agents).unwrap();
    assert!(engine.authorize_mode(&id, Mode::Local).is_err());
}

#[test]
fn contract_mismatch_rolls_back_creation_and_blocks_claim() {
    let (dir, mut engine, id) = setup();
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute(
        "UPDATE run_contracts SET expected_md5='tampered' WHERE run_id=?1",
        [&id],
    )
    .unwrap();
    assert!(engine.claim(&id, 100, 30).is_err());
    assert_eq!(engine.counts(&id).unwrap().pending, 3);
    conn.execute_batch("DROP TRIGGER execution_contracts_no_UPDATE; UPDATE execution_contracts SET sha256='tampered';").unwrap();
    let spec: ResearchSpec =
        serde_json::from_str(include_str!("../examples/research.json")).unwrap();
    assert!(engine.create(spec).is_err());
    assert_eq!(engine.list().unwrap().len(), 1);
}

#[derive(Clone)]
struct FakeClock {
    base: i64,
    millis: Arc<AtomicU64>,
}
impl FakeClock {
    fn new() -> Self {
        Self {
            base: researchops::engine::now(),
            millis: Arc::new(AtomicU64::new(0)),
        }
    }
}
impl Clock for FakeClock {
    fn now(&self) -> i64 {
        self.base + (self.millis.load(Ordering::SeqCst) / 1000) as i64
    }
    fn elapsed(&self) -> Duration {
        Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
    async fn sleep(&self, duration: Duration) {
        self.millis
            .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn runner_continues_three_tasks_at_batch_one_and_finalizes_readback() {
    let (dir, mut engine, id) = setup();
    bind(&mut engine, &id, "research", 900, 1);
    drop(engine);
    let result = execution::execute(
        dir.path().join("test.db"),
        id.clone(),
        Mode::Local,
        Trigger::Manual,
        None,
        SystemClock::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.business_status, "COMPLETE");
    assert_eq!(result.system_status, "completed");
    let engine = Engine::open(dir.path().join("test.db")).unwrap();
    let view = engine.execution_view(&id).unwrap();
    assert_eq!(view.counts.readback_verified, 3);
    assert_eq!(view.counts.succeeded, 3);
    assert!(view.gate_failures.is_empty());
}

#[tokio::test]
async fn committed_result_without_receipt_recovers_without_redelivery() {
    let (dir, mut engine, id) = setup();
    let at = researchops::engine::now();
    let claim = engine.claim(&id, at, 300).unwrap().unwrap();
    let evidence = engine.extract(&claim).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER injected_receipt_failure BEFORE INSERT ON batch_readbacks BEGIN SELECT RAISE(ABORT,'injected post-commit failure'); END;").unwrap();
    assert!(engine.finish(&claim, &evidence, at).is_err());
    assert_eq!(engine.counts(&id).unwrap().succeeded, 1);
    assert_eq!(engine.counts(&id).unwrap().readback_verified, 0);
    conn.execute_batch("DROP TRIGGER injected_receipt_failure;")
        .unwrap();
    drop(engine);
    let result = execution::execute(
        dir.path().join("test.db"),
        id.clone(),
        Mode::Local,
        Trigger::Recovery,
        None,
        SystemClock::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.business_status, "COMPLETE");
    let engine = Engine::open(dir.path().join("test.db")).unwrap();
    assert_eq!(engine.get(&id).unwrap().tasks[0].attempts, 1);
    assert_eq!(engine.counts(&id).unwrap().readback_verified, 3);
}

#[tokio::test]
async fn persistent_post_commit_receipt_failure_records_the_exact_stage() {
    let (dir, engine, id) = setup();
    drop(engine);
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER injected_receipt_failure BEFORE INSERT ON batch_readbacks BEGIN SELECT RAISE(ABORT,'injected post-commit receipt failure'); END;").unwrap();
    let result = execution::execute(
        dir.path().join("test.db"),
        id,
        Mode::Local,
        Trigger::Manual,
        None,
        SystemClock::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.business_status, "PARTIAL");
    assert_eq!(result.failure_stage.as_deref(), Some("checkpoint_readback"));
    assert_eq!(
        result.last_successful_stage.as_deref(),
        Some("checkpoint_commit")
    );
    assert!(result
        .error
        .unwrap()
        .contains("injected post-commit receipt failure"));
    let counts = result.after.unwrap();
    assert_eq!(counts.succeeded, 1);
    assert_eq!(counts.readback_verified, 0);
}

#[tokio::test]
async fn zero_claim_waits_for_a_lease_and_never_means_complete() {
    let (dir, mut engine, id) = setup();
    let clock = FakeClock::new();
    let claimed = engine.claim(&id, clock.now(), 3).unwrap().unwrap();
    drop(engine);
    let result = execution::execute(
        dir.path().join("test.db"),
        id.clone(),
        Mode::Local,
        Trigger::Recovery,
        None,
        clock.clone(),
    )
    .await
    .unwrap();
    assert_eq!(result.business_status, "COMPLETE");
    assert!(clock.elapsed() >= Duration::from_secs(3));
    let engine = Engine::open(dir.path().join("test.db")).unwrap();
    assert_eq!(
        engine
            .get(&id)
            .unwrap()
            .tasks
            .iter()
            .find(|t| t.id == claimed.task_id)
            .unwrap()
            .attempts,
        2
    );
}

#[tokio::test]
async fn future_eligibility_past_budget_preserves_partial_resume_position() {
    let (dir, mut engine, id) = setup();
    bind(&mut engine, &id, "research", 1, 1);
    let clock = FakeClock::new();
    engine.claim(&id, clock.now(), 100).unwrap();
    drop(engine);
    let result = execution::execute(
        dir.path().join("test.db"),
        id,
        Mode::Local,
        Trigger::Scheduled,
        None,
        clock,
    )
    .await
    .unwrap();
    assert_eq!(result.business_status, "PARTIAL");
    assert_eq!(result.stop_code, "next_eligible_after_budget");
    assert!(result.resume["next_eligible_at"].is_number());
    assert_eq!(result.after.unwrap().succeeded, 2);
}

#[test]
fn session_ownership_and_stale_finalization_are_fenced() {
    let (_dir, mut engine, id) = setup();
    let at = researchops::engine::now();
    let old = engine
        .begin_execution(&id, Mode::Local, Trigger::Manual, at)
        .unwrap();
    assert!(engine
        .begin_execution(&id, Mode::Local, Trigger::Scheduled, at)
        .is_err());
    let new = engine
        .begin_execution(&id, Mode::Local, Trigger::Recovery, at + 301)
        .unwrap();
    assert_ne!(old.id, new.id);
    assert!(engine
        .touch_execution(&old, None, "late", at + 302, 0)
        .is_err());
    assert!(engine
        .finish_execution(&old, "late", None, None, at + 302, 0)
        .is_err());
}

#[test]
fn transient_sql_retries_are_bounded_and_semantic_failures_are_not_replayed() {
    let mut calls = 0;
    let value = execution::sql_retry(2, || -> anyhow::Result<()> {
        calls += 1;
        Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("busy".into()),
        )
        .into())
    });
    assert!(value.is_err());
    assert_eq!(calls, 3);
    let mut calls = 0;
    let value = execution::sql_retry(2, || -> anyhow::Result<()> {
        calls += 1;
        anyhow::bail!("contract mismatch")
    });
    assert!(value.is_err());
    assert_eq!(calls, 1);
}

#[tokio::test]
async fn weekly_external_proofs_and_missing_provider_remain_blocked() {
    for (profile, mode, expected) in [
        (
            "weekly_report",
            Mode::Local,
            "required_external_adapters_unavailable",
        ),
        (
            "model_reviewed_research",
            Mode::Agents,
            "provider_unavailable",
        ),
    ] {
        let (dir, mut engine, id) = setup();
        bind(&mut engine, &id, profile, 10, 1);
        drop(engine);
        let result = execution::execute(
            dir.path().join("test.db"),
            id.clone(),
            mode,
            Trigger::Manual,
            None,
            SystemClock::default(),
        )
        .await
        .unwrap();
        assert_eq!(result.business_status, "PARTIAL");
        assert_eq!(result.stop_code, expected);
        assert_eq!(result.after.unwrap().pending, 3);
    }
}

#[tokio::test]
async fn manual_scheduled_and_recovery_share_completion_rules() {
    for trigger in [Trigger::Manual, Trigger::Scheduled, Trigger::Recovery] {
        let (dir, engine, id) = setup();
        drop(engine);
        let result = execution::execute(
            dir.path().join("test.db"),
            id.clone(),
            Mode::Local,
            trigger,
            None,
            SystemClock::default(),
        )
        .await
        .unwrap();
        assert_eq!(result.business_status, "COMPLETE");
        assert_eq!(result.after.unwrap().readback_verified, 3);
        let engine = Engine::open(dir.path().join("test.db")).unwrap();
        assert_eq!(
            engine.latest_session(&id).unwrap().unwrap().trigger,
            trigger.as_str()
        );
    }
}

#[tokio::test]
async fn old_artifacts_and_bounded_termination_cannot_establish_completion() {
    let (dir, mut engine, id) = setup();
    engine.work(&id, 100, 60).unwrap();
    engine.materialize_report(&id).unwrap();
    assert_ne!(
        engine.execution_view(&id).unwrap().business_status,
        "COMPLETE"
    );
    drop(engine);
    execution::execute(
        dir.path().join("test.db"),
        id.clone(),
        Mode::Local,
        Trigger::Manual,
        None,
        SystemClock::default(),
    )
    .await
    .unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute(
        "UPDATE tasks SET question='Changed input' WHERE id=(SELECT id FROM tasks WHERE run_id=?1 ORDER BY rowid LIMIT 1)",
        [&id],
    )
    .unwrap();
    let engine = Engine::open(dir.path().join("test.db")).unwrap();
    let view = engine.execution_view(&id).unwrap();
    assert_ne!(view.business_status, "COMPLETE");
    assert!(view
        .gate_failures
        .iter()
        .any(|g| g == "storage_integrity_failure"));
}

#[tokio::test]
async fn database_failure_preserves_recovery_json_and_does_not_invent_counts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-a-database");
    std::fs::create_dir(&path).unwrap();
    let result = execution::execute(
        path.clone(),
        "unavailable-run".into(),
        Mode::Local,
        Trigger::Recovery,
        None,
        SystemClock::default(),
    )
    .await
    .unwrap();
    assert!(!result.database_recorded);
    assert!(result.before.is_none());
    assert!(result.after.is_none());
    assert!(result.recovery_file.is_some());
    let files = execution::read_recovery_files(&path, Some("unavailable-run"));
    assert_eq!(files.len(), 1);
    assert!(files[0].resume["sql"].as_str().unwrap().contains("?1"));
    let blocker = dir.path().join("blocked");
    std::fs::write(&blocker, "file").unwrap();
    let result = execution::execute(
        blocker.join("db.sqlite"),
        "failed-file".into(),
        Mode::Local,
        Trigger::Manual,
        None,
        SystemClock::default(),
    )
    .await
    .unwrap();
    assert!(!result.database_recorded);
    assert!(result.recovery_file.is_none());
    assert!(result.recovery_file_error.is_some());
}

#[test]
fn diagnostics_redact_auth_headers_and_private_addresses() {
    let text=execution::redact("provider failed\nAuthorization: Bearer private-token\nrecipient private@example.com sk-secret",None);
    assert!(!text.contains("private-token"));
    assert!(!text.contains("private@example.com"));
    assert!(!text.contains("sk-secret"));
    assert_eq!(execution::kst_date(0), "1970-01-01");
    assert_eq!(execution::kst_date(15 * 3600), "1970-01-02");
}

#[derive(Clone)]
struct TickClock {
    base: i64,
    ticks: Arc<AtomicU64>,
}
impl Clock for TickClock {
    fn now(&self) -> i64 {
        self.base + (self.ticks.load(Ordering::SeqCst) / 1000) as i64
    }
    fn elapsed(&self) -> Duration {
        Duration::from_millis(self.ticks.fetch_add(100, Ordering::SeqCst))
    }
    async fn sleep(&self, d: Duration) {
        self.ticks.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn concrete_budget_exhaustion_records_elapsed_limit_and_remaining_work() {
    let (dir, mut engine, id) = setup();
    bind(&mut engine, &id, "research", 1, 1);
    drop(engine);
    let clock = TickClock {
        base: researchops::engine::now(),
        ticks: Arc::new(AtomicU64::new(0)),
    };
    let result = execution::execute(
        dir.path().join("test.db"),
        id,
        Mode::Local,
        Trigger::Manual,
        None,
        clock,
    )
    .await
    .unwrap();
    assert_eq!(result.stop_code, "execution_budget_exhausted");
    assert_eq!(result.budget_seconds, Some(1));
    assert!(result.elapsed_ms >= 1000);
    assert_eq!(result.business_status, "PARTIAL");
    assert!(result.after.unwrap().pending > 0);
    assert!(result.failure_stage.is_some());
    assert!(result.last_successful_stage.is_some());
}
