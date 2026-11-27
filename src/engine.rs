use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    pub title: String,
    #[serde(default)]
    pub uri: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchSpec {
    pub objective: String,
    pub questions: Vec<String>,
    pub sources: Vec<SourceInput>,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub source_id: String,
    pub source_title: String,
    pub source_uri: String,
    pub source_hash: String,
    pub quote: String,
    pub score: f64,
    pub verification: String,
}

#[derive(Debug, Serialize)]
pub struct TaskView {
    pub id: String,
    pub question: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct RunView {
    pub id: String,
    pub objective: String,
    pub created_at: i64,
    pub status: String,
    pub total_questions: usize,
    pub processed_questions: usize,
    pub covered_questions: usize,
    pub tasks: Vec<TaskView>,
}

#[derive(Debug)]
pub struct Claim {
    pub task_id: String,
    pub run_id: String,
    pub question: String,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct WorkResult {
    pub processed: usize,
    pub run: RunView,
}

pub struct Engine {
    conn: Connection,
}

impl Engine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))?;
        Ok(Self { conn })
    }

    pub fn create(&mut self, spec: ResearchSpec) -> Result<String> {
        validate(&spec)?;
        let id = Uuid::new_v4().to_string();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO runs(id,objective,created_at) VALUES(?1,?2,?3)",
            params![id, spec.objective.trim(), now()],
        )?;
        for q in spec.questions {
            tx.execute(
                "INSERT OR IGNORE INTO tasks(id,run_id,question) VALUES(?1,?2,?3)",
                params![Uuid::new_v4().to_string(), id, q.trim()],
            )?;
        }
        for source in spec.sources {
            tx.execute("INSERT OR IGNORE INTO sources(id,run_id,title,uri,content,content_hash) VALUES(?1,?2,?3,?4,?5,?6)",
                params![Uuid::new_v4().to_string(),id,source.title.trim(),source.uri,source.content,hash(&source.content)])?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn list(&self) -> Result<Vec<RunView>> {
        let ids = self
            .conn
            .prepare("SELECT id FROM runs ORDER BY created_at DESC,rowid DESC LIMIT 100")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| self.get(id)).collect()
    }

    pub fn get(&self, id: &str) -> Result<RunView> {
        let (objective, created_at) = self
            .conn
            .query_row(
                "SELECT objective,created_at FROM runs WHERE id=?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
            .context("research run not found")?;
        let mut tasks = self.conn.prepare("SELECT id,question,status,attempts,last_error FROM tasks WHERE run_id=?1 ORDER BY rowid")?
            .query_map([id], |r| Ok(TaskView {id:r.get(0)?,question:r.get(1)?,status:r.get(2)?,attempts:r.get(3)?,last_error:r.get(4)?,findings:vec![]}))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for task in &mut tasks {
            task.findings = self.conn.prepare("SELECT s.id,s.title,s.uri,s.content_hash,f.quote,f.score,f.verification FROM findings f JOIN sources s ON s.id=f.source_id WHERE f.task_id=?1 ORDER BY f.score DESC,s.title,s.id")?
                .query_map([&task.id], |r| Ok(Finding {source_id:r.get(0)?,source_title:r.get(1)?,source_uri:r.get(2)?,source_hash:r.get(3)?,quote:r.get(4)?,score:r.get(5)?,verification:r.get(6)?}))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        let processed = tasks
            .iter()
            .filter(|t| t.status == "succeeded" || t.status == "failed")
            .count();
        let covered = tasks.iter().filter(|t| !t.findings.is_empty()).count();
        let status = if processed == tasks.len() {
            if covered == tasks.len() && tasks.iter().all(|t| t.status == "succeeded") {
                "complete"
            } else {
                "partial"
            }
        } else if tasks.iter().all(|t| t.attempts == 0) {
            "pending"
        } else {
            "running"
        };
        Ok(RunView {
            id: id.into(),
            objective,
            created_at,
            status: status.into(),
            total_questions: tasks.len(),
            processed_questions: processed,
            covered_questions: covered,
            tasks,
        })
    }

    // Every delivery gets a new fencing token. Expired workers cannot commit.
    pub fn claim(&mut self, run_id: &str, at: i64, lease_seconds: i64) -> Result<Option<Claim>> {
        if !(1..=3600).contains(&lease_seconds) {
            bail!("lease_seconds must be 1..3600");
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE tasks SET status='failed',lease_token=NULL,lease_until=NULL,last_error='retry limit reached after worker lease expired' WHERE run_id=?1 AND status='running' AND lease_until<=?2 AND attempts>=3", params![run_id,at])?;
        let row = tx.query_row("SELECT id,question FROM tasks WHERE run_id=?1 AND attempts<3 AND ((status='pending' AND available_at<=?2) OR (status='running' AND lease_until<=?2)) ORDER BY rowid LIMIT 1", params![run_id,at], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?;
        let claim = if let Some((task_id, question)) = row {
            let token = Uuid::new_v4().to_string();
            tx.execute("UPDATE tasks SET status='running',attempts=attempts+1,lease_token=?2,lease_until=?3 WHERE id=?1",params![task_id,token,at+lease_seconds])?;
            Some(Claim {
                task_id,
                run_id: run_id.into(),
                question,
                token,
            })
        } else {
            None
        };
        tx.commit()?;
        Ok(claim)
    }

    pub fn extract(&self, claim: &Claim) -> Result<Vec<Finding>> {
        let sources = self.conn.prepare("SELECT id,title,uri,content,content_hash FROM sources WHERE run_id=?1 ORDER BY rowid")?
            .query_map([&claim.run_id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let terms = tokens(&claim.question);
        let mut findings = Vec::new();
        for (id, title, uri, content, content_hash) in sources {
            let best = content
                .lines()
                .flat_map(|line| line.split_inclusive(['.', '!', '?', '。']))
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.chars().count() <= 1200)
                .map(|quote| {
                    let words = tokens(quote);
                    let count = terms.intersection(&words).count();
                    (quote, count)
                })
                .filter(|(_, count)| *count > 0)
                .max_by_key(|(_, count)| *count);
            if let Some((quote, count)) = best {
                findings.push(Finding {
                    source_id: id,
                    source_title: title,
                    source_uri: uri,
                    source_hash: content_hash,
                    quote: quote.into(),
                    score: count as f64 / terms.len().max(1) as f64,
                    verification: "exact_quote".into(),
                });
            }
        }
        Ok(findings)
    }

    pub fn finish(&mut self, claim: &Claim, findings: &[Finding], at: i64) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute("UPDATE tasks SET status='succeeded',lease_token=NULL,lease_until=NULL,last_error=NULL WHERE id=?1 AND run_id=?2 AND status='running' AND lease_token=?3 AND lease_until>?4", params![claim.task_id,claim.run_id,claim.token,at])?;
        if changed != 1 {
            bail!("lease expired or claim superseded; results rejected");
        }
        for finding in findings {
            let (content, stored_hash) = tx.query_row(
                "SELECT content,content_hash FROM sources WHERE id=?1 AND run_id=?2",
                params![finding.source_id, claim.run_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?;
            if finding.quote.trim().is_empty()
                || !content.contains(&finding.quote)
                || hash(&content) != stored_hash
                || finding.source_hash != stored_hash
            {
                bail!("evidence does not match the stored source snapshot");
            }
            tx.execute("INSERT OR IGNORE INTO findings(task_id,source_id,quote,score,verification) VALUES(?1,?2,?3,?4,'exact_quote')",params![claim.task_id,finding.source_id,finding.quote,finding.score])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn fail(&mut self, claim: &Claim, message: &str, at: i64) -> Result<()> {
        let changed = self.conn.execute("UPDATE tasks SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'pending' END,available_at=?4+(1 << attempts),last_error=?5,lease_token=NULL,lease_until=NULL WHERE id=?1 AND run_id=?2 AND status='running' AND lease_token=?3 AND lease_until>?4",params![claim.task_id,claim.run_id,claim.token,at,message])?;
        if changed != 1 {
            bail!("lease expired or claim superseded; failure update rejected");
        }
        Ok(())
    }

    pub fn work(&mut self, id: &str, limit: usize, lease_seconds: i64) -> Result<WorkResult> {
        if !(1..=1000).contains(&limit) {
            bail!("limit must be 1..1000");
        }
        self.get(id)?;
        let mut processed = 0;
        for _ in 0..limit {
            let Some(claim) = self.claim(id, now(), lease_seconds)? else {
                break;
            };
            match self.extract(&claim) {
                Ok(findings) => self.finish(&claim, &findings, now())?,
                Err(err) => self.fail(&claim, &err.to_string(), now())?,
            }
            processed += 1;
        }
        Ok(WorkResult {
            processed,
            run: self.get(id)?,
        })
    }

    pub fn report(&self, id: &str) -> Result<String> {
        let run = self.get(id)?;
        let mut out = format!("# ResearchOps evidence report\n\n**Objective:** {}\n\nRun: `{}` · Status: **{}**\n\nProcessed: {}/{} questions · Lexical evidence coverage: {}/{} questions\n\n> This is extractive keyword retrieval over supplied snapshots. An exact quote proves text provenance, not factual truth or that the question is answered. Coverage is limited to supplied documents; no independent web audit or LLM synthesis has run.\n", md(&run.objective), run.id,run.status,run.processed_questions,run.total_questions,run.covered_questions,run.total_questions);
        for task in run.tasks {
            out.push_str(&format!(
                "\n## {}\n\nTask: {} · Attempts: {}\n",
                md(&task.question),
                task.status,
                task.attempts
            ));
            if task.findings.is_empty() {
                out.push_str("\n**Gap:** no matching evidence recorded. Review source coverage or refine the question.\n");
            }
            if let Some(error) = task.last_error {
                out.push_str(&format!("\nError: {}\n", md(&error)));
            }
            for f in task.findings {
                out.push_str(&format!("\n### Source: {}\n\n{}\n\n> {}\n\nEvidence: `{}` · Keyword overlap: {:.0}%\n\nSnapshot SHA-256: `{}`\n",md(&f.source_title),md(&f.source_uri),f.quote.lines().map(md).collect::<Vec<_>>().join("\n> "),f.verification,f.score*100.0,f.source_hash));
            }
        }
        Ok(out)
    }
}

fn md(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(['\r', '\n'], " ")
        .replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('`', "\\`")
        .replace('#', "\\#")
}

fn tokens(s: &str) -> BTreeSet<String> {
    const STOP: &[&str] = &[
        "a", "an", "the", "is", "are", "was", "were", "what", "which", "how", "why", "does", "do",
        "of", "to", "in", "on", "and", "or", "for", "with", "by", "it", "this", "that", "can",
        "be", "as", "from", "about", "을", "를", "이", "가", "은", "는",
    ];
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 1 && !STOP.contains(t))
        .map(String::from)
        .collect()
}

fn validate(spec: &ResearchSpec) -> Result<()> {
    if spec.objective.trim().is_empty() || spec.objective.len() > 4000 {
        bail!("objective must contain 1..4000 bytes");
    }
    if spec.questions.is_empty() || spec.questions.len() > 100 {
        bail!("provide 1..100 questions");
    }
    if spec
        .questions
        .iter()
        .any(|q| q.trim().is_empty() || q.len() > 1000 || tokens(q).is_empty())
    {
        bail!("questions must contain searchable terms and be at most 1000 bytes");
    }
    if spec.sources.is_empty() || spec.sources.len() > 100 {
        bail!("provide 1..100 source snapshots");
    }
    if spec.sources.iter().any(|s| {
        s.title.trim().is_empty()
            || s.title.len() > 500
            || s.uri.len() > 2000
            || s.content.trim().is_empty()
            || s.content.len() > 1_000_000
    }) {
        bail!("each source needs a title (1..500 bytes), URI <=2000 bytes and content (1..1000000 bytes)");
    }
    if spec.sources.iter().map(|s| s.content.len()).sum::<usize>() > 5_000_000 {
        bail!("total source content must not exceed 5 MB");
    }
    Ok(())
}
