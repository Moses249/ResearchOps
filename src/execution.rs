use crate::{
    contracts::{Mode, Policy, Profile, Trigger},
    engine::{now, Claim, Engine},
    provider::Provider,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    future::Future,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub trait Clock: Clone + Send + Sync + 'static {
    fn now(&self) -> i64;
    fn elapsed(&self) -> Duration;
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send;
}
#[derive(Clone)]
pub struct SystemClock(Instant);
impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for SystemClock {
    fn now(&self) -> i64 {
        now()
    }
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

pub fn kst_date(at: i64) -> String {
    DateTime::<Utc>::from_timestamp(at, 0)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .with_timezone(&FixedOffset::east_opt(9 * 3600).expect("valid offset"))
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Counts {
    pub total: i64,
    pub pending: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub covered: i64,
    pub readback_verified: i64,
    pub model_reviewed: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionView {
    pub id: String,
    pub mode: String,
    pub trigger: String,
    pub system_status: String,
    pub business_status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub start_kst_date: String,
    pub end_kst_date: Option<String>,
    pub elapsed_ms: i64,
    pub lease_until: i64,
    pub budget_seconds: i64,
    pub before: Counts,
    pub after: Option<Counts>,
    pub stop_code: Option<String>,
    pub last_successful_stage: Option<String>,
    pub failure_stage: Option<String>,
    pub error: Option<String>,
    pub resume: Option<Value>,
}
#[derive(Debug, Serialize)]
pub struct ExecutionView {
    pub business_status: String,
    pub completion_scope: String,
    pub gate_failures: Vec<String>,
    pub counts: Counts,
    pub latest: Option<SessionView>,
    pub resume: Value,
}
#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub run_id: String,
    pub policy: Policy,
    pub before: Counts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub run_id: String,
    pub session_id: Option<String>,
    pub system_status: String,
    pub business_status: String,
    pub stop_code: String,
    pub start_kst_date: String,
    pub end_kst_date: String,
    pub elapsed_ms: u64,
    pub budget_seconds: Option<u64>,
    pub before: Option<Counts>,
    pub after: Option<Counts>,
    pub last_observed_counts: Option<Counts>,
    pub last_successful_stage: Option<String>,
    pub failure_stage: Option<String>,
    pub error: Option<String>,
    pub resume: Value,
    pub database_recorded: bool,
    pub recovery_file: Option<String>,
    pub recovery_file_error: Option<String>,
}

pub fn sql_retry<T>(retries: usize, mut operation: impl FnMut() -> Result<T>) -> Result<T> {
    for attempt in 0..=retries.min(2) {
        match operation() {
            Ok(value) => return Ok(value),
            Err(err) => {
                let transient=err.chain().any(|cause|matches!(cause.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(code,_)) if matches!(code.code,rusqlite::ErrorCode::DatabaseBusy|rusqlite::ErrorCode::DatabaseLocked)));
                if !transient || attempt == retries.min(2) {
                    return Err(err);
                }
                std::thread::sleep(Duration::from_millis(20 * (1 << attempt)));
            }
        }
    }
    unreachable!()
}

pub(crate) async fn db<T: Send + 'static>(
    path: &Path,
    f: impl FnOnce(&mut Engine) -> Result<T> + Send + 'static,
) -> Result<T> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut engine = sql_retry(2, || Engine::open(&path))?;
        f(&mut engine)
    })
    .await?
}

pub fn resume_position(run_id: &str, next: Option<i64>) -> Value {
    json!({"run_id":run_id,"sql":"SELECT id,question,status,attempts,available_at,lease_until,last_error FROM tasks WHERE run_id=?1 AND status!='succeeded' ORDER BY (attempts>0) DESC,rowid","parameters":[run_id],"readback_sql":"SELECT b.id,b.task_id FROM execution_batches b LEFT JOIN batch_readbacks r ON b.id=r.batch_id WHERE b.run_id=?1 AND r.batch_id IS NULL","next_eligible_at":next})
}

impl Engine {
    pub fn counts(&self, id: &str) -> Result<Counts> {
        let mut c=self.conn.query_row("SELECT count(*),coalesce(sum(status='pending'),0),coalesce(sum(status='running'),0),coalesce(sum(status='succeeded'),0),coalesce(sum(status='failed'),0) FROM tasks WHERE run_id=?1",[id],|r|Ok(Counts{total:r.get(0)?,pending:r.get(1)?,running:r.get(2)?,succeeded:r.get(3)?,failed:r.get(4)?,..Default::default()}))?;
        c.covered=self.conn.query_row("SELECT count(DISTINCT t.id) FROM tasks t JOIN findings f ON t.id=f.task_id WHERE t.run_id=?1",[id],|r|r.get(0))?;
        c.readback_verified=self.conn.query_row("SELECT count(*) FROM execution_batches b JOIN batch_readbacks r ON b.id=r.batch_id AND b.sha256=r.sha256 AND b.findings_count=r.findings_count AND r.task_status='succeeded' WHERE b.run_id=?1",[id],|r|r.get(0))?;
        c.model_reviewed=self.conn.query_row("SELECT count(*) FROM model_review_receipts m JOIN tasks t ON m.task_id=t.id WHERE t.run_id=?1",[id],|r|r.get(0))?;
        Ok(c)
    }

    pub fn next_available(&self, id: &str) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT min(CASE WHEN status='pending' THEN available_at ELSE lease_until END) FROM tasks WHERE run_id=?1 AND status IN ('pending','running')",[id],|r|r.get(0))?)
    }

    pub fn latest_session(&self, id: &str) -> Result<Option<SessionView>> {
        let raw=self.conn.query_row("SELECT id,mode,trigger_kind,system_status,business_status,started_at,finished_at,start_kst_date,end_kst_date,elapsed_ms,lease_until,budget_seconds,before_counts,after_counts,stop_code,last_successful_stage,failure_stage,error,resume_json FROM execution_sessions WHERE run_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1",[id],|r|Ok(SessionView{id:r.get(0)?,mode:r.get(1)?,trigger:r.get(2)?,system_status:r.get(3)?,business_status:r.get(4)?,started_at:r.get(5)?,finished_at:r.get(6)?,start_kst_date:r.get(7)?,end_kst_date:r.get(8)?,elapsed_ms:r.get(9)?,lease_until:r.get(10)?,budget_seconds:r.get(11)?,before:serde_json::from_str(&r.get::<_,String>(12)?).map_err(|e|rusqlite::Error::FromSqlConversionFailure(12,rusqlite::types::Type::Text,Box::new(e)))?,after:r.get::<_,Option<String>>(13)?.map(|s|serde_json::from_str(&s)).transpose().map_err(|e|rusqlite::Error::FromSqlConversionFailure(13,rusqlite::types::Type::Text,Box::new(e)))?,stop_code:r.get(14)?,last_successful_stage:r.get(15)?,failure_stage:r.get(16)?,error:r.get(17)?,resume:r.get::<_,Option<String>>(18)?.map(|s|serde_json::from_str(&s)).transpose().map_err(|e|rusqlite::Error::FromSqlConversionFailure(18,rusqlite::types::Type::Text,Box::new(e)))?})).optional()?;
        Ok(raw)
    }

    fn evaluate(&self, id: &str, finalizing: bool) -> Result<ExecutionView> {
        let counts = self.counts(id)?;
        let latest = self.latest_session(id)?;
        let mut gates = Vec::new();
        let profile = match self.checked_policy(id) {
            Ok(p) => Some(p.profile),
            Err(e) => {
                gates.push(format!("contract_invalid: {e}"));
                None
            }
        };
        if counts.total == 0 || counts.pending + counts.running > 0 {
            gates.push("unfinished_tasks".into());
        }
        if counts.failed > 0 {
            gates.push("failed_tasks".into());
        }
        if counts.covered < counts.total {
            gates.push("evidence_gaps".into());
        }
        if counts.readback_verified < counts.succeeded {
            gates.push("storage_readback_missing".into());
        }
        if self.integrity_check(id).is_err() {
            gates.push("storage_integrity_failure".into());
        }
        if !self.current_artifact_verified(id).unwrap_or(false) {
            gates.push("current_report_readback_missing".into());
        }
        let mode: Option<String> = self
            .conn
            .query_row(
                "SELECT mode FROM run_contracts WHERE run_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if (profile == Some(Profile::ModelReviewedResearch) || mode.as_deref() == Some("agents"))
            && counts.model_reviewed < counts.total
        {
            gates.push("model_review_missing".into());
        }
        if profile == Some(Profile::WeeklyReport) {
            gates.extend(
                [
                    "live_source_ranking_adapter_unavailable",
                    "pinned_template_unverified",
                    "rendered_manifest_unverified",
                    "gmail_sent_receipt_unverified",
                ]
                .map(String::from),
            );
        }
        if !finalizing {
            let finalized = if let Ok(fingerprint) = self.input_fingerprint(id) {
                self.conn.query_row("SELECT EXISTS(SELECT 1 FROM execution_sessions WHERE run_id=?1 AND system_status='completed' AND business_status='COMPLETE' AND final_input_sha256=?2)",params![id,fingerprint],|r|r.get::<_,bool>(0))?
            } else {
                false
            };
            if !finalized {
                gates.push("runner_finalization_missing".into());
            }
        }
        let status = if gates.is_empty() {
            "COMPLETE"
        } else if latest
            .as_ref()
            .is_some_and(|s| s.system_status == "running" && s.lease_until > now())
            || (latest.is_none() && counts.succeeded + counts.failed == 0)
        {
            "IN_PROGRESS"
        } else {
            "PARTIAL"
        };
        Ok(ExecutionView {
            business_status: status.into(),
            completion_scope: if profile == Some(Profile::WeeklyReport) {
                "weekly_report"
            } else {
                "supplied_documents"
            }
            .into(),
            gate_failures: gates,
            counts,
            latest,
            resume: resume_position(id, self.next_available(id)?),
        })
    }
    pub fn execution_view(&self, id: &str) -> Result<ExecutionView> {
        self.evaluate(id, false)
    }

    pub fn begin_execution(
        &mut self,
        id: &str,
        mode: Mode,
        trigger: Trigger,
        at: i64,
    ) -> Result<Session> {
        let policy = self.authorize_mode(id, mode)?;
        let before = self.counts(id)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE execution_sessions SET system_status='interrupted',business_status='PARTIAL',finished_at=?2,end_kst_date=?3,stop_code='session_lease_expired',failure_stage='session_lease',error='Lease expired; interruption cause is unknown',after_counts=?4,resume_json=?5 WHERE run_id=?1 AND system_status='running' AND lease_until<=?2",params![id,at,kst_date(at),serde_json::to_string(&before)?,serde_json::to_string(&resume_position(id,None))?])?;
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM execution_sessions WHERE run_id=?1 AND system_status='running')",[id],|r|r.get(0))?;
        if active {
            bail!("execution session already active");
        }
        let session = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO execution_sessions(id,run_id,mode,trigger_kind,system_status,business_status,started_at,start_kst_date,lease_until,budget_seconds,before_counts) VALUES(?1,?2,?3,?4,'running','IN_PROGRESS',?5,?6,?7,?8,?9)",params![session,id,mode.as_str(),trigger.as_str(),at,kst_date(at),at+300,policy.max_runtime_seconds,serde_json::to_string(&before)?])?;
        tx.commit()?;
        Ok(Session {
            id: session,
            run_id: id.into(),
            policy,
            before,
        })
    }

    pub fn touch_execution(
        &mut self,
        session: &Session,
        claim: Option<&Claim>,
        _stage: &str,
        at: i64,
        elapsed_ms: u64,
    ) -> Result<()> {
        self.checked_policy(&session.run_id)?;
        let counts = self.counts(&session.run_id)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed=tx.execute("UPDATE execution_sessions SET lease_until=?2,after_counts=?3,elapsed_ms=?4 WHERE id=?1 AND system_status='running' AND lease_until>?5",params![session.id,at+300,serde_json::to_string(&counts)?,elapsed_ms,at])?;
        if changed != 1 {
            bail!("session lease expired or superseded");
        }
        if let Some(c) = claim {
            if c.run_id != session.run_id {
                bail!("claim outside session scope");
            }
            let changed=tx.execute("UPDATE tasks SET lease_until=?4 WHERE id=?1 AND run_id=?2 AND lease_token=?3 AND status='running' AND lease_until>?5",params![c.task_id,c.run_id,c.token,at+300,at])?;
            if changed != 1 {
                bail!("task lease expired or superseded");
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn finish_execution(
        &mut self,
        session: &Session,
        stop: &str,
        stage: Option<&str>,
        error: Option<&str>,
        at: i64,
        elapsed: u64,
    ) -> Result<Diagnostic> {
        let evaluated = self.evaluate(&session.run_id, true)?;
        let business = if evaluated.gate_failures.is_empty() && error.is_none() {
            "COMPLETE"
        } else {
            "PARTIAL"
        };
        let last = self
            .latest_session(&session.run_id)?
            .context("session missing")?;
        let result = Diagnostic {
            run_id: session.run_id.clone(),
            session_id: Some(session.id.clone()),
            system_status: if error.is_some() {
                "failed"
            } else {
                "completed"
            }
            .into(),
            business_status: business.into(),
            stop_code: stop.into(),
            start_kst_date: last.start_kst_date,
            end_kst_date: kst_date(at),
            elapsed_ms: elapsed,
            budget_seconds: Some(session.policy.max_runtime_seconds),
            before: Some(last.before),
            after: Some(evaluated.counts),
            last_observed_counts: None,
            last_successful_stage: last.last_successful_stage,
            failure_stage: stage.map(String::from),
            error: error.map(String::from),
            resume: evaluated.resume,
            database_recorded: true,
            recovery_file: None,
            recovery_file_error: None,
        };
        let fingerprint = self.input_fingerprint(&session.run_id).ok();
        let changed=self.conn.execute("UPDATE execution_sessions SET system_status=?2,business_status=?3,finished_at=?4,end_kst_date=?5,elapsed_ms=?6,after_counts=?7,stop_code=?8,failure_stage=?9,error=?10,resume_json=?11,final_input_sha256=?12 WHERE id=?1 AND system_status='running' AND lease_until>?4",params![session.id,result.system_status,result.business_status,at,result.end_kst_date,elapsed,serde_json::to_string(&result.after)?,stop,stage,error,serde_json::to_string(&result.resume)?,fingerprint])?;
        if changed != 1 {
            bail!("session lease expired or superseded; finalization rejected");
        }
        let observed:(String,String,Option<String>)=self.conn.query_row("SELECT system_status,business_status,final_input_sha256 FROM execution_sessions WHERE id=?1",[&session.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if observed.0 != result.system_status
            || observed.1 != result.business_status
            || observed.2 != fingerprint
        {
            bail!("session finalization readback mismatch");
        }
        Ok(result)
    }

    pub fn recovery_candidates(&self) -> Result<Vec<Value>> {
        let ids = self
            .conn
            .prepare("SELECT id FROM runs ORDER BY created_at,rowid")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut result = Vec::new();
        for id in ids {
            let view = self.execution_view(&id)?;
            if view.business_status != "COMPLETE" {
                result.push(json!({"run_id":id,"execution":view}));
            }
        }
        Ok(result)
    }

    pub(crate) fn defer_claim(&mut self, claim: &Claim, at: i64) -> Result<()> {
        let changed=self.conn.execute("UPDATE tasks SET status='pending',lease_token=NULL,lease_until=NULL,available_at=?4,last_error='execution budget exhausted' WHERE id=?1 AND run_id=?2 AND lease_token=?3 AND status='running' AND lease_until>?4",params![claim.task_id,claim.run_id,claim.token,at])?;
        if changed != 1 {
            bail!("cannot defer expired claim");
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct BudgetExceeded;
impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("execution budget exhausted")
    }
}
impl std::error::Error for BudgetExceeded {}

#[derive(Clone)]
pub(crate) struct Control<C: Clock> {
    pub path: PathBuf,
    pub session: Option<Session>,
    pub clock: C,
    pub started: Duration,
    pub budget: Duration,
    pub last_success: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}
impl<C: Clock> Control<C> {
    pub fn elapsed_ms(&self) -> u64 {
        self.clock
            .elapsed()
            .saturating_sub(self.started)
            .as_millis()
            .min(u64::MAX as u128) as u64
    }
    pub fn remaining(&self) -> Result<Duration> {
        let left = self
            .budget
            .saturating_sub(self.clock.elapsed().saturating_sub(self.started));
        if left.is_zero() {
            Err(BudgetExceeded.into())
        } else {
            Ok(left)
        }
    }
    pub async fn guard(&self, claim: Option<&Claim>, stage: &str) -> Result<()> {
        self.remaining()?;
        if let Some(session) = self.session.clone() {
            let c = claim.cloned();
            let at = self.clock.now();
            let elapsed = self.elapsed_ms();
            let stage = stage.to_string();
            db(&self.path, move |engine| {
                let retries = session.policy.sql_retries;
                sql_retry(retries, || {
                    engine.touch_execution(&session, c.as_ref(), &stage, at, elapsed)
                })
            })
            .await?;
        }
        Ok(())
    }

    pub async fn success(&self, stage: &str) -> Result<()> {
        if let Some(session) = self.session.clone() {
            let stage = stage.to_string();
            let at = self.clock.now();
            let elapsed = self.elapsed_ms();
            db(&self.path,move |e|{e.touch_execution(&session,None,&stage,at,elapsed)?;e.conn.execute("UPDATE execution_sessions SET last_successful_stage=?2 WHERE id=?1 AND system_status='running' AND lease_until>?3",params![session.id,stage,at])?;Ok(())}).await?;
        }
        *self.last_success.lock().expect("progress lock") = Some(stage.into());
        Ok(())
    }
}

pub async fn execute<C: Clock>(
    path: PathBuf,
    id: String,
    mode: Mode,
    trigger: Trigger,
    provider: Option<Provider>,
    clock: C,
) -> Result<Diagnostic> {
    let started = clock.elapsed();
    let start_at = clock.now();
    let run = id.clone();
    let initialization = db(&path, move |engine| {
        engine.begin_execution(&run, mode, trigger, start_at)
    })
    .await;
    let session = match initialization {
        Ok(session) => session,
        Err(error) => {
            // Invalid requests never become successful executions or mutate task state.
            let message = redact(&error.to_string(), provider.as_ref());
            let mut diagnostic = Diagnostic {
                run_id: id.clone(),
                session_id: None,
                system_status: "failed".into(),
                business_status: "PARTIAL".into(),
                stop_code: if message.contains("already active") {
                    "execution_active"
                } else {
                    "contract_or_storage_preflight_failed"
                }
                .into(),
                start_kst_date: kst_date(start_at),
                end_kst_date: kst_date(clock.now()),
                elapsed_ms: clock.elapsed().saturating_sub(started).as_millis() as u64,
                budget_seconds: None,
                before: None,
                after: None,
                last_observed_counts: None,
                last_successful_stage: None,
                failure_stage: Some("preflight".into()),
                error: Some(message),
                resume: resume_position(&id, None),
                database_recorded: false,
                recovery_file: None,
                recovery_file_error: None,
            };
            persist_recovery(&path, &mut diagnostic);
            return Ok(diagnostic);
        }
    };
    let control = Control {
        path: path.clone(),
        session: Some(session.clone()),
        clock: clock.clone(),
        started,
        budget: Duration::from_secs(session.policy.max_runtime_seconds),
        last_success: Default::default(),
    };
    let mut last_counts = None;
    let work = run_loop(
        &control,
        &session,
        mode,
        provider.as_ref(),
        &mut last_counts,
    )
    .await;
    let (stop, stage, error) = match work {
        Ok(stop) => {
            let stage = if stop == "required_work_complete" {
                None
            } else {
                Some(
                    match stop.as_str() {
                        "provider_unavailable" => "provider_preflight",
                        "next_eligible_after_budget" => "waiting_for_eligible_work",
                        "required_external_adapters_unavailable" => "external_proof_gate",
                        _ => "completion_gate",
                    }
                    .to_string(),
                )
            };
            (stop, stage, None)
        }
        Err((stage, error)) => {
            let stop = if error.is::<BudgetExceeded>() {
                "execution_budget_exhausted"
            } else if error.to_string().contains("integrity")
                || error.to_string().contains("mismatch")
            {
                "integrity_failure"
            } else {
                "storage_or_execution_failure"
            };
            let message = if error.is::<BudgetExceeded>() {
                None
            } else {
                Some(redact(&format!("{error:#}"), provider.as_ref()))
            };
            (stop.into(), Some(stage), message)
        }
    };
    let s = session.clone();
    let code = stop.clone();
    let failure = stage.clone();
    let message = error.clone();
    let at = clock.now();
    let elapsed = control.elapsed_ms();
    let finalized = db(&path, move |engine| {
        engine.finish_execution(
            &s,
            &code,
            failure.as_deref(),
            message.as_deref(),
            at,
            elapsed,
        )
    })
    .await;
    match finalized {
        Ok(value) => Ok(value),
        Err(err) => {
            let mut result = Diagnostic {
                run_id: id,
                session_id: Some(session.id),
                system_status: "failed".into(),
                business_status: "PARTIAL".into(),
                stop_code: "storage_failure".into(),
                start_kst_date: kst_date(start_at),
                end_kst_date: kst_date(at),
                elapsed_ms: elapsed,
                budget_seconds: Some(session.policy.max_runtime_seconds),
                before: Some(session.before.clone()),
                after: None,
                last_observed_counts: last_counts.or(Some(session.before.clone())),
                last_successful_stage: control.last_success.lock().expect("progress lock").clone(),
                failure_stage: Some(stage.unwrap_or_else(|| "session_finalization".into())),
                error: Some(redact(
                    &format!("{}; finalization: {err:#}", error.unwrap_or(stop)),
                    provider.as_ref(),
                )),
                resume: resume_position(&session.run_id, None),
                database_recorded: false,
                recovery_file: None,
                recovery_file_error: None,
            };
            persist_recovery(&path, &mut result);
            Ok(result)
        }
    }
}

async fn run_loop<C: Clock>(
    control: &Control<C>,
    session: &Session,
    mode: Mode,
    provider: Option<&Provider>,
    last_counts: &mut Option<Counts>,
) -> std::result::Result<String, (String, anyhow::Error)> {
    async fn inner<C: Clock>(
        c: &Control<C>,
        s: &Session,
        mode: Mode,
        p: Option<&Provider>,
        observed: &mut Option<Counts>,
        stage: &mut String,
    ) -> Result<String> {
        c.success("contract_checked").await?;
        if s.policy.profile == Profile::WeeklyReport {
            return Ok("required_external_adapters_unavailable".into());
        }
        let initial = e_counts(&c.path, &s.run_id).await?;
        if mode == Mode::Agents
            && initial.pending + initial.running > 0
            && p.is_none_or(|provider| !provider.tested)
        {
            return Ok("provider_unavailable".into());
        }
        if mode == Mode::Agents {
            crate::agents::preflight(&c.path, &s.run_id).await?;
        }
        *stage = "readback_recovery".into();
        let run = s.run_id.clone();
        let retries = s.policy.sql_retries;
        db(&c.path, move |e| {
            sql_retry(retries, || e.recover_readbacks(&run))
        })
        .await?;
        c.success("readback_recovery").await?;
        let mut in_batch = 0;
        loop {
            *stage = "queue_claim".into();
            c.guard(None, stage).await?;
            let run = s.run_id.clone();
            let at = c.clock.now();
            let retries = s.policy.sql_retries;
            let claim = db(&c.path, move |e| {
                sql_retry(retries, || e.claim(&run, at, 300))
            })
            .await?;
            let Some(claim) = claim else {
                let run = s.run_id.clone();
                let (counts, next) = db(&c.path, move |e| {
                    Ok((e.counts(&run)?, e.next_available(&run)?))
                })
                .await?;
                *observed = Some(counts.clone());
                if counts.pending + counts.running == 0 {
                    break;
                }
                let due = next.context("unfinished work has no eligible time")?;
                let wait = Duration::from_secs((due - c.clock.now()).max(1) as u64);
                if wait >= c.remaining()? {
                    return Ok("next_eligible_after_budget".into());
                }
                *stage = "waiting_for_eligible_work".into();
                c.clock.sleep(wait.min(Duration::from_secs(30))).await;
                continue;
            };
            *stage = "task_processing".into();
            let result = if mode == Mode::Local {
                let task = claim.clone();
                let at = c.clock.now();
                db(&c.path, move |e| {
                    let evidence = e.extract(&task)?;
                    e.finish(&task, &evidence, at)
                })
                .await
            } else {
                crate::agents::process_in_session(c, p.context("provider missing")?, claim.clone())
                    .await
            };
            if let Err(error) = result {
                if error.is::<BudgetExceeded>() {
                    let task = claim.clone();
                    let at = c.clock.now();
                    let _ = db(&c.path, move |e| e.defer_claim(&task, at)).await;
                    return Err(error);
                }
                // A commit may already exist. Repair readback; never rerun its side effect.
                let task = claim.task_id.clone();
                let run = s.run_id.clone();
                let recovered = db(&c.path, move |e| {
                    e.recover_readbacks(&run)?;
                    Ok(e.conn.query_row(
                        "SELECT status='succeeded' FROM tasks WHERE id=?1",
                        [task],
                        |r| r.get::<_, bool>(0),
                    )?)
                })
                .await;
                if !matches!(recovered, Ok(true)) {
                    return Err(error);
                }
            }
            *stage = "checkpoint_readback".into();
            c.guard(None, stage).await?;
            let run = s.run_id.clone();
            *observed = Some(db(&c.path, move |e| e.counts(&run)).await?);
            c.success("checkpoint_readback").await?;
            in_batch += 1;
            if in_batch == s.policy.batch_size {
                in_batch = 0;
                tokio::task::yield_now().await;
            }
        }
        *stage = "report_materialization".into();
        c.guard(None, stage).await?;
        let run = s.run_id.clone();
        db(&c.path, move |e| e.materialize_report(&run)).await?;
        c.success("report_readback").await?;
        *stage = "completion_gate".into();
        c.guard(None, stage).await?;
        let run = s.run_id.clone();
        let view = db(&c.path, move |e| e.evaluate(&run, true)).await?;
        Ok(if view.counts.failed > 0 {
            "retry_limit_reached"
        } else if view.counts.covered < view.counts.total {
            "evidence_gaps"
        } else if !view.gate_failures.is_empty() {
            "completion_gates_unmet"
        } else {
            "required_work_complete"
        }
        .into())
    }
    let mut stage = "preflight".to_string();
    inner(control, session, mode, provider, last_counts, &mut stage)
        .await
        .map_err(|err| (stage, err))
}

async fn e_counts(path: &Path, id: &str) -> Result<Counts> {
    let run = id.to_string();
    db(path, move |e| e.counts(&run)).await
}

pub fn redact(message: &str, provider: Option<&Provider>) -> String {
    let text = provider
        .map(|p| p.redact(message))
        .unwrap_or_else(|| message.to_string());
    let lines = text
        .lines()
        .map(|line| {
            if let Some(index) = line.to_ascii_lowercase().find("authorization:") {
                format!("{}Authorization: [REDACTED]", &line[..index])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    lines
        .split_whitespace()
        .map(|word| {
            if word.starts_with("sk-") || word.starts_with("ghp_") || word.starts_with("gho_") {
                "[REDACTED]"
            } else if word.contains('@') && word.contains('.') {
                "[REDACTED_EMAIL]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(6000)
        .collect()
}

pub fn persist_recovery(db_path: &Path, diagnostic: &mut Diagnostic) {
    let root = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory = root.join("recovery");
    let result = (|| -> Result<PathBuf> {
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.json", Uuid::new_v4()));
        let temporary = directory.join(format!("{}.tmp", Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(diagnostic)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, &path)?;
        Ok(path)
    })();
    match result {
        Ok(path) => diagnostic.recovery_file = Some(path.display().to_string()),
        Err(err) => {
            diagnostic.recovery_file_error = Some(format!(
                "recovery file could not be saved: {}",
                err.root_cause()
            ))
        }
    }
}

pub fn read_recovery_files(db_path: &Path, run_id: Option<&str>) -> Vec<Diagnostic> {
    let root = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let Ok(entries) = std::fs::read_dir(root.join("recovery")) else {
        return vec![];
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json")
            || entry
                .metadata()
                .map(|m| m.len() > 1_000_000)
                .unwrap_or(true)
        {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(mut record) = serde_json::from_slice::<Diagnostic>(&bytes) {
                if run_id.is_some_and(|id| id != record.run_id) {
                    continue;
                }
                record.error = record.error.map(|e| redact(&e, None));
                let next = record.resume["next_eligible_at"].as_i64();
                record.resume = resume_position(&record.run_id, next);
                record.recovery_file = Some(path.display().to_string());
                files.push(record);
            }
        }
    }
    files.sort_by(|a, b| b.end_kst_date.cmp(&a.end_kst_date));
    files.truncate(100);
    files
}
