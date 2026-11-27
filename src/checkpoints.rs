use crate::{
    contracts::md5_digest,
    engine::{hash, Claim, Engine},
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use uuid::Uuid;

pub(crate) fn task_snapshot(conn: &Connection, id: &str) -> Result<Value> {
    let (question, status): (String, String) =
        conn.query_row("SELECT question,status FROM tasks WHERE id=?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let answer: Option<String> = conn
        .query_row(
            "SELECT answer FROM agent_answers WHERE task_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let review: Option<String> = conn
        .query_row(
            "SELECT answer_sha256 FROM model_review_receipts WHERE task_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let findings=conn.prepare("SELECT f.source_id,f.quote,f.score,s.content,s.content_hash FROM findings f JOIN sources s ON s.id=f.source_id WHERE f.task_id=?1 ORDER BY f.source_id,f.quote")?.query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,f64>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut evidence = Vec::new();
    for (source, quote, score, content, digest) in findings {
        if hash(&content) != digest || quote.trim().is_empty() || !content.contains(&quote) {
            bail!("source integrity mismatch");
        }
        evidence
            .push(json!({"source_id":source,"quote":quote,"score":score,"source_sha256":digest}));
    }
    if let Some(review_hash) = &review {
        if answer.as_ref().map(|a| hash(a)).as_ref() != Some(review_hash) {
            bail!("model review integrity mismatch");
        }
    }
    Ok(
        json!({"task_id":id,"question":question,"status":status,"answer":answer,"model_review_sha256":review,"evidence":evidence}),
    )
}
use rusqlite::OptionalExtension;

pub(crate) fn append_checkpoint(conn: &Connection, claim: &Claim, at: i64) -> Result<()> {
    let snapshot = task_snapshot(conn, &claim.task_id)?;
    let payload = serde_json::to_string(&snapshot)?;
    conn.execute("INSERT INTO execution_batches(id,run_id,task_id,lease_token,payload,sha256,md5,findings_count,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![Uuid::new_v4().to_string(),claim.run_id,claim.task_id,claim.token,payload,hash(&payload),md5_digest(&payload),snapshot["evidence"].as_array().context("checkpoint evidence missing")?.len() as i64,at])?;
    Ok(())
}

impl Engine {
    pub fn verify_checkpoint(&mut self, task_id: &str) -> Result<()> {
        let (id, payload, sha, md5, count): (String, String, String, String, i64) = self
            .conn
            .query_row(
            "SELECT id,payload,sha256,md5,findings_count FROM execution_batches WHERE task_id=?1",
            [task_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        let observed = task_snapshot(&self.conn, task_id)?;
        let actual = serde_json::to_string(&observed)?;
        if actual != payload
            || hash(&payload) != sha
            || md5_digest(&payload) != md5
            || observed["evidence"]
                .as_array()
                .context("evidence missing")?
                .len() as i64
                != count
        {
            bail!("checkpoint readback mismatch");
        }
        self.conn.execute("INSERT OR IGNORE INTO batch_readbacks(batch_id,sha256,findings_count,task_status,checked_at) VALUES(?1,?2,?3,?4,?5)",params![id,sha,count,observed["status"].as_str(),crate::engine::now()])?;
        let valid:bool=self.conn.query_row("SELECT sha256=?2 AND findings_count=?3 AND task_status='succeeded' FROM batch_readbacks WHERE batch_id=?1",params![id,sha,count],|r|r.get(0))?;
        if !valid {
            bail!("stored readback receipt mismatch");
        }
        Ok(())
    }

    pub fn recover_readbacks(&mut self, run_id: &str) -> Result<()> {
        let ids=self.conn.prepare("SELECT b.task_id FROM execution_batches b LEFT JOIN batch_readbacks r ON r.batch_id=b.id WHERE b.run_id=?1 AND r.batch_id IS NULL ORDER BY b.created_at,b.rowid")?.query_map([run_id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for id in ids {
            self.verify_checkpoint(&id)?;
        }
        Ok(())
    }

    pub(crate) fn integrity_check(&self, run_id: &str) -> Result<()> {
        let batches=self.conn.prepare("SELECT b.task_id,b.payload,b.sha256,b.md5,b.findings_count,r.sha256,r.findings_count,r.task_status FROM execution_batches b LEFT JOIN batch_readbacks r ON r.batch_id=b.id WHERE b.run_id=?1")?.query_map([run_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<i64>>(6)?,r.get::<_,Option<String>>(7)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for (task, payload, sha, md5, count, receipt_sha, receipt_count, receipt_status) in batches
        {
            let observed = task_snapshot(&self.conn, &task)?;
            if serde_json::to_string(&observed)? != payload
                || hash(&payload) != sha
                || md5_digest(&payload) != md5
                || observed["evidence"]
                    .as_array()
                    .context("evidence missing")?
                    .len() as i64
                    != count
            {
                bail!("checkpoint integrity mismatch");
            }
            if receipt_sha.as_ref().is_some_and(|r| r != &sha)
                || receipt_count.is_some_and(|r| r != count)
                || receipt_status.as_deref().is_some_and(|s| s != "succeeded")
            {
                bail!("readback receipt integrity mismatch");
            }
        }
        Ok(())
    }

    pub fn input_fingerprint(&self, run_id: &str) -> Result<String> {
        let contract:Option<(String,String,String,Option<String>)>=self.conn.query_row("SELECT version,expected_sha256,expected_md5,mode FROM run_contracts WHERE run_id=?1",[run_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let objective: String =
            self.conn
                .query_row("SELECT objective FROM runs WHERE id=?1", [run_id], |r| {
                    r.get(0)
                })?;
        let ids = self
            .conn
            .prepare("SELECT id FROM tasks WHERE run_id=?1 ORDER BY id")?
            .query_map([run_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let tasks = ids
            .iter()
            .map(|id| task_snapshot(&self.conn, id))
            .collect::<Result<Vec<_>>>()?;
        let mut sources = Vec::new();
        for source in self.snapshots(run_id)? {
            if hash(&source.content) != source.content_hash {
                bail!("source integrity mismatch");
            }
            sources.push(json!({"id":source.id,"title":source.title,"uri":source.uri,"sha256":source.content_hash}));
        }
        sources.sort_by_key(|s| s["id"].as_str().unwrap_or_default().to_string());
        Ok(hash(&serde_json::to_string(
            &json!({"objective":objective,"contract":contract,"sources":sources,"tasks":tasks}),
        )?))
    }

    pub fn materialize_report(&mut self, run_id: &str) -> Result<String> {
        let body = self.report(run_id)?;
        let input = self.input_fingerprint(run_id)?;
        let digest = hash(&body);
        self.conn.execute("INSERT OR IGNORE INTO report_artifacts(id,run_id,body,sha256,input_sha256,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![Uuid::new_v4().to_string(),run_id,body,digest,input,crate::engine::now()])?;
        let (id,observed,sha,fingerprint):(String,String,String,String)=self.conn.query_row("SELECT id,body,sha256,input_sha256 FROM report_artifacts WHERE run_id=?1 AND input_sha256=?2 AND sha256=?3",params![run_id,input,digest],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        if observed != body
            || hash(&observed) != sha
            || fingerprint != self.input_fingerprint(run_id)?
        {
            bail!("report readback mismatch");
        }
        self.conn.execute("INSERT OR IGNORE INTO artifact_readbacks(artifact_id,sha256,input_sha256,checked_at) VALUES(?1,?2,?3,?4)",params![id,sha,fingerprint,crate::engine::now()])?;
        Ok(body)
    }

    pub(crate) fn current_artifact_verified(&self, run_id: &str) -> Result<bool> {
        let input = self.input_fingerprint(run_id)?;
        let rows=self.conn.prepare("SELECT a.body,a.sha256,r.sha256,r.input_sha256 FROM report_artifacts a JOIN artifact_readbacks r ON a.id=r.artifact_id WHERE a.run_id=?1 AND a.input_sha256=?2 ORDER BY a.created_at DESC,a.rowid DESC")?.query_map(params![run_id,input],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .any(|(body, sha, receipt, fp)| hash(&body) == sha && sha == receipt && fp == input))
    }
}
