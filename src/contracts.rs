use crate::engine::{hash, Engine};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const DEFAULT_VERSION: &str = "researchops-evidence-v1";
pub const DEFAULT_BODY: &str =
    r#"{"profile":"research","max_runtime_seconds":900,"batch_size":20,"sql_retries":2}"#;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Local,
    Agents,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Agents => "agents",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Manual,
    Scheduled,
    Recovery,
}
impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
            Self::Recovery => "recovery",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Research,
    ModelReviewedResearch,
    WeeklyReport,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub profile: Profile,
    pub max_runtime_seconds: u64,
    pub batch_size: usize,
    pub sql_retries: usize,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if !(1..=3600).contains(&self.max_runtime_seconds)
            || !(1..=100).contains(&self.batch_size)
            || self.sql_retries > 2
        {
            bail!(
                "invalid contract limits: runtime 1..3600 seconds, batch 1..100, SQL retries 0..2"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractImport {
    pub version: String,
    pub body: String,
    pub expected_md5: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub version: String,
    pub expected_md5: String,
}

pub fn md5_digest(body: &str) -> String {
    format!("{:x}", md5::compute(body.as_bytes()))
}

pub(crate) fn install_default(conn: &Connection, at: i64) -> Result<()> {
    conn.execute("INSERT INTO execution_contracts(version,body,sha256,md5,created_at) VALUES(?1,?2,?3,?4,?5)",params![DEFAULT_VERSION,DEFAULT_BODY,hash(DEFAULT_BODY),md5_digest(DEFAULT_BODY),at])?;
    Ok(())
}

pub(crate) fn bind_default(conn: &Connection, run_id: &str) -> Result<()> {
    let (body, sha, md5): (String, String, String) = conn.query_row(
        "SELECT body,sha256,md5 FROM execution_contracts WHERE version=?1",
        [DEFAULT_VERSION],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if body != DEFAULT_BODY || sha != hash(DEFAULT_BODY) || md5 != md5_digest(DEFAULT_BODY) {
        bail!("default contract integrity mismatch");
    }
    conn.execute("INSERT INTO run_contracts(run_id,version,expected_sha256,expected_md5) VALUES(?1,?2,?3,?4)",params![run_id,DEFAULT_VERSION,sha,md5])?;
    Ok(())
}

impl Engine {
    pub fn register_contract(&mut self, input: ContractImport) -> Result<()> {
        if input.version.trim().is_empty()
            || input.version.len() > 100
            || input.body.len() > 100_000
        {
            bail!("invalid contract version/body size");
        }
        if input.expected_md5 != md5_digest(&input.body) {
            bail!("contract MD5 mismatch");
        }
        let policy: Policy =
            serde_json::from_str(&input.body).context("unsupported contract JSON policy")?;
        policy.validate()?;
        // Registration retries use the same version/content, never overwrite a version.
        let existing: Option<(String, String, String)> = self
            .conn
            .query_row(
                "SELECT body,sha256,md5 FROM execution_contracts WHERE version=?1",
                [&input.version],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((body, sha, md5)) = existing {
            if body == input.body && sha == hash(&body) && md5 == input.expected_md5 {
                return Ok(());
            }
            bail!("contract version already exists with different or invalid content");
        }
        self.conn.execute("INSERT INTO execution_contracts(version,body,sha256,md5,created_at) VALUES(?1,?2,?3,?4,?5)",params![input.version,input.body,hash(&input.body),input.expected_md5,crate::engine::now()])?;
        let observed: String = self.conn.query_row(
            "SELECT body FROM execution_contracts WHERE version=?1",
            [input.version],
            |r| r.get(0),
        )?;
        if observed != input.body {
            bail!("contract readback mismatch");
        }
        Ok(())
    }

    pub fn contracts(&self) -> Result<Vec<serde_json::Value>> {
        Ok(self.conn.prepare("SELECT version,md5,sha256,body FROM execution_contracts ORDER BY created_at,version")?.query_map([],|r|Ok(serde_json::json!({"version":r.get::<_,String>(0)?,"md5":r.get::<_,String>(1)?,"sha256":r.get::<_,String>(2)?,"body":r.get::<_,String>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn checked_policy(&self, run_id: &str) -> Result<Policy> {
        let row:Option<(String,String,String,String,String)>=self.conn.query_row("SELECT c.body,c.sha256,c.md5,r.expected_sha256,r.expected_md5 FROM run_contracts r JOIN execution_contracts c ON c.version=r.version WHERE r.run_id=?1",[run_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let (body, sha, md5, expected_sha, expected_md5) =
            row.context("contract not bound; legacy results have no execution-contract proof")?;
        if hash(&body) != sha
            || md5_digest(&body) != md5
            || sha != expected_sha
            || md5 != expected_md5
        {
            bail!("execution contract digest mismatch");
        }
        let policy: Policy = serde_json::from_str(&body).context("unsupported contract policy")?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn bind_contract(&mut self, run_id: &str, binding: Binding) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (count, attempted): (i64, i64) = tx.query_row(
            "SELECT count(*),coalesce(sum(attempts),0) FROM tasks WHERE run_id=?1",
            [run_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let sessions: i64 = tx.query_row(
            "SELECT count(*) FROM execution_sessions WHERE run_id=?1",
            [run_id],
            |r| r.get(0),
        )?;
        let pinned: Option<String> = tx
            .query_row(
                "SELECT mode FROM run_contracts WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if count == 0 || attempted > 0 || sessions > 0 || pinned.is_some() {
            bail!("contract binding is allowed only before execution");
        }
        let (body, sha, md5): (String, String, String) = tx.query_row(
            "SELECT body,sha256,md5 FROM execution_contracts WHERE version=?1",
            [&binding.version],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if hash(&body) != sha || md5_digest(&body) != md5 || binding.expected_md5 != md5 {
            bail!("contract binding digest mismatch");
        }
        let policy: Policy = serde_json::from_str(&body)?;
        policy.validate()?;
        tx.execute("INSERT INTO run_contracts(run_id,version,expected_sha256,expected_md5) VALUES(?1,?2,?3,?4) ON CONFLICT(run_id) DO UPDATE SET version=excluded.version,expected_sha256=excluded.expected_sha256,expected_md5=excluded.expected_md5",params![run_id,binding.version,sha,md5])?;
        tx.commit()?;
        Ok(())
    }

    pub fn authorize_mode(&mut self, run_id: &str, mode: Mode) -> Result<Policy> {
        let policy = self.checked_policy(run_id)?;
        if policy.profile == Profile::ModelReviewedResearch && mode != Mode::Agents {
            bail!("contract requires agent model review");
        }
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<String> = tx.query_row(
            "SELECT mode FROM run_contracts WHERE run_id=?1",
            [run_id],
            |r| r.get(0),
        )?;
        if current
            .as_deref()
            .is_some_and(|value| value != mode.as_str())
        {
            bail!("run execution mode is already pinned");
        }
        tx.execute(
            "UPDATE run_contracts SET mode=?2 WHERE run_id=?1 AND mode IS NULL",
            params![run_id, mode.as_str()],
        )?;
        tx.commit()?;
        Ok(policy)
    }
}
