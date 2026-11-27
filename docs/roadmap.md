# ResearchOps roadmap

## v0.1 — local evidence workspace

- Rust CLI, embedded web dashboard, JSON API.
- SQLite durable tasks, lease fencing, bounded retries.
- Supplied document snapshots, keyword retrieval, quote provenance.
- Coverage gaps and Markdown export.
- Recovery and concurrency tests.

Acceptance: the bundled example survives restart, reports two covered questions and one explicit gap, and rejects stale or tampered worker results.

## v0.2 — useful source ingestion

- Text/Markdown file import and one structured API adapter.
- Source retrieval timestamps, deduplication and changed-source tracking.
- Adapter timeouts, request budgets, rate limits and error classification.
- Worker daemon with bounded polling, cancellation and lease renewal.

Acceptance: one real research request runs from ingestion to report; interrupted work recovers; network failures and exhausted budgets appear explicitly.

## v0.3 — evidence-aware evaluation

- Optional model provider with structured outputs.
- Claim-to-passage links and exact evidence offsets.
- Separate retrieval score, support status, contradictions and uncertainty.
- Human review and a small labeled evaluation dataset.

Acceptance: every report claim is linked to evidence or marked unsupported; quality is evaluated independently of citation count.

## v0.4 — coverage and independent audit

- Explicit source/topic/time scope matrix.
- Auditor with a different discovery strategy.
- Bounded follow-up tasks and auditable stop reasons.
- Second domain example without changing engine behavior.

Acceptance: known omissions in fixtures are surfaced, and the report distinguishes searched scope from unsearched scope.

## v0.5 — distributed operation

- PostgreSQL backend and separate worker processes.
- Metrics, traces, queue age and recovery latency.
- Deployment, backup/restore and operational benchmarks.
- Kubernetes only after a repeatable container deployment and demonstrated need.

Acceptance: recovery and concurrency invariants pass against PostgreSQL, and recorded fault-injection results explain practical limits.
