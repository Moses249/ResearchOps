# Architecture — MVP v0.1

## Decision: Rust core, SQLite first

The first release demonstrates the objective → task → evidence → report lifecycle on a single machine. SQLite removes database provisioning from the demo while retaining transactional claims and durable recovery. PostgreSQL and multiple hosts are deliberately deferred until the local invariants are exercised. Existing Supabase automation is not accessed or modified.

## Execution path

```text
Dashboard / CLI / JSON API
           |
      ResearchSpec
           |
   immutable run + snapshots
           |
   one task per unique question
           |
   transactional lease claim
           |
   keyword extraction outside transaction
           |
   token + expiry + snapshot verification
           |
   findings + completion in one transaction
           |
   coverage summary / Markdown report
```

The web layer opens a SQLite connection within each blocking job. Worker operations execute through Tokio's blocking pool so extraction and SQLite calls do not block the async network executor. The engine is shared by the CLI and API.

## Task state machine

```text
pending --claim--> running --valid evidence commit--> succeeded
                     |
                     +--processing error--> pending (backoff; attempts < 3)
                     +--processing error--> failed  (attempts = 3)
                     +--lease expiry------> claimable / failed at retry limit
```

A task can succeed with zero findings. That records a completed attempt with an evidence gap, and the run becomes partial once all tasks are terminal. Failed tasks also force a partial run. Completion status is derived from database rows rather than a model response.

## Invariants

1. A live lease has one current token. Every new delivery replaces it.
2. Completion requires the current token and a strictly unexpired lease.
3. Findings and task completion commit together or both roll back.
4. Evidence must be a nonempty exact substring of a snapshot whose hash matches.
5. A snapshot referenced by evidence belongs to the same run.
6. Reprocessing cannot duplicate the same task/source/quote finding.
7. A task has at most three deliveries; retries do not run before their backoff.
8. Required work with no evidence is never silently represented as fully covered.

## Research quality boundary

Lexical matching and quote provenance do not establish entailment, truth, source independence, or research completeness. No free-form answer is generated in this release. The report exposes candidate passages and missing coverage for human review. Source text is treated as data and is never executed as instructions.

The synthetic example has two supported topic questions and one deliberately absent benchmark question. It is a functional fixture, not a live research-quality benchmark.

## Extension order

1. Introduce explicit adapter interfaces when adding the second ingestion mechanism.
2. Add source retrieval timestamps, fetch policy, and bounded HTTP retrieval.
3. Add provider-independent structured extraction with evidence offsets and validation.
4. Separate verified support, contradictions, and human review from lexical retrieval.
5. Add a planned coverage matrix and an auditor using a distinct search strategy.
6. Add PostgreSQL claims, heartbeat renewal, and worker daemon before multi-host deployment.

Avoid building a general plugin loader or a second workflow runtime in advance of those requirements.
