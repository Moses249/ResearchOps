> **phase:** implement · Design + tasks approved 2026-11-28 · owner @Moses249

# Feature: execution-completion-contract

English translation of the [Korean feature plan](execution-completion-contract.md). Both versions share AC IDs and phase; this is not a separate approval. References: [Project/Plan](../project/Plan.en.md), [Project/Design](../project/Design.en.md).

## Plan

### Summary

Keep research execution progressing beyond small batches and intermediate saves. Continue according to explicit contracts, or preserve a concrete partial-result reason and resume position. Manual and future scheduled invocations use the same Runner; system invocation termination and business completion remain distinct.

### Problem and baseline

The MVP already provides a SQL queue, expired-lease recovery, token-based stale-write protection, exact-quote checks, and a model review loop. Per-call task limits and in-memory progress do not consistently establish post-write verification, current-result completion, or recovery evidence after interruption.

Translate the common persistence requirements of the supplied operational contracts into product behavior. The existing execution-contract draft is reference material to reconcile against approved criteria and design. PR #1's verification is not verification of this feature.

### Goals / non-goals

Provide version-pinned contracts, durable sessions/checkpoints, unfinished-work-first recovery, continuation across batches, SQL readback gates, and inspectable failure/resume information.

Exclude live job discovery/ranking, the fixed HTML renderer, Gmail sending, production Supabase changes, scheduler installation, multi-host execution, prevention of platform termination, and interpreting source instructions as executable SQL/code.

### User scenarios

1. Recover unfinished state and missing storage receipts before continuing eligible work.
2. Distinguish delayed retries, active leases, failures, and evidence gaps even when a claim returns no rows.
3. Inspect measured counts, exact failure stages, parameterized recovery SQL, and safe recovery JSON after a database error or configured limit.
4. Keep business completion blocked when role-specific proof is missing, even after every task has been processed.

### Acceptance criteria

| ID | Verifiable condition |
| --- | --- |
| **AC-1** | Before execution, SELECT the stored contract version, body, and expected digest. Reject mismatches, unsupported policies, and invalid limits before claiming work, calling models, or delivering results. Distinguish the built-in local contract from explicit new-version registration. Do not interpret external text as instructions. |
| **AC-2** | Batch size is not an overall stop condition. With three eligible tasks and batch size one, one session processes, persists, and reads back all three unless another contractual blocker applies. Invoke the next eligible task/stage immediately after each successful task or batch. |
| **AC-3** | Within the selected run/mode scope, recover missing readbacks and retried/expired work before fresh tasks. Never execute another research run implicitly. Verify SQL claim priority and rejection of expired/stale-token writes under concurrency and recovery. |
| **AC-4** | Source, observation/checkpoint, receipt, and artifact ledgers are append-only. Commit results, checkpoint, and queue transition together; require a separate post-commit SELECT receipt checking hashes, counts, and state. Reject direct ledger UPDATE/DELETE and surface tampered evidence as an integrity failure. |
| **AC-5** | Zero claims do not establish completion. Distinguish pending work, live running leases, delayed retries, failures, and missing evidence through SQL and expose the next eligible time. Process other eligible work first and continue delayed work within the session's budget. |
| **AC-6** | Expose system-session and business states separately through API, CLI, and UI. Business COMPLETE requires all required work, evidence, storage readbacks, and a report artifact matching current inputs. Contracts requiring model review need actual approval results. Worker termination alone never establishes completion. Preserve the legacy `status` field while explicitly exposing business completion in a new field. |
| **AC-7** | Retry classified transient SQL failures such as busy/locked at most twice after the initial attempt. Do not blindly replay non-idempotent writes, already committed results, or delivery. Recover interruption between commit and receipt through actual readback without duplicating results. |
| **AC-8** | A PARTIAL record includes start/end KST dates, observed before/after counts, last successful stage, failure stage, actual redacted error, concrete stop code, run ID, SQL and bindings, and next eligible time. Preserve equivalent local recovery JSON when the database cannot store it, without claiming persistence succeeded. |
| **AC-9** | Use an explicit execution budget: proposed defaults are 900 seconds, batch size 20, and two SQL retries; maximum budget is 3600 seconds. Record configured limits, measured elapsed time, and remaining counts on budget exhaustion. A batch count or vague lack of time is not a valid stop code. Record expired leases without inferring the interruption cause. |
| **AC-10** | Test that manual, scheduled, and recovery triggers use the same Runner and completion gate. Do not install a scheduler. Observe unfinished sessions and scoped recovery candidates from the database after restart; indicate reconnection when required API credentials are unavailable. |
| **AC-11** | Verify migrations and historical-result queries preserve the existing MVP database. Do not invent missing historical readbacks, model approvals, or delivery proof. Old artifacts and reports inconsistent with current inputs cannot satisfy a new completion gate. |
| **AC-12** | Block policies requiring external SQL, ranking, templates, rendered manifests, or Gmail proof when adapters/evidence are unavailable. Fixtures cannot establish production completion. Do not expose keys, auth headers, or private recipient settings through public responses, logs, recovery JSON, or Git. Provide keyboard-accessible and textual progress, gates, errors, and resume positions. |

### Dependencies and decisions

- Use the approved project Plan/Design and existing Rust/SQLite/API baseline.
- Create the implementation branch from main after baseline PR #1 is merged. This Plan does not perform or authorize that merge.
- Validate SQLite first; PostgreSQL, live Supabase, and Gmail are follow-up features.
- Scope is the selected supplied-document research run and execution mode. Country/role queues belong to their adapter features.
- Pin supported structured policies by version/digest; do not execute prose contracts. External legacy contract/template checks remain blocked until actually connected.
- Feature Design will specify compatibility between the shared Runner and legacy bounded work APIs. The dashboard's actual research execution must use the shared Runner.
- No milestone or release version is assigned. No scope-affecting questions remain; the proposed defaults are part of the Plan approval.

### Verification approach

Use a fake clock and failure injection for budgets, leases, and retries; verify actual transactions, SELECTs, and append-only constraints in temporary databases. Use a mock model provider. Compare UI/API/CLI state and recovery information. A fresh read-only QA agent independently verifies every AC. These tests do not establish production job completion.

## Design

### Impact — implementation read contract

| Files | Change |
| --- | --- |
| `src/contracts.rs` (new) | Typed policy validation, limits, version/digest registry, run binding |
| `src/checkpoints.rs` (new) | Atomic checkpoints, actual SQL readback, input fingerprints, artifacts, idempotent recovery |
| `src/execution.rs` (replace draft) | Shared Runner, sessions, clock, budget, waiting/resume, completion evaluation, recovery JSON |
| `src/engine.rs` | Versioned migration, atomic run/binding creation, scoped claims, persistence boundary, legacy compatibility |
| `src/agents.rs` | Extract single-claim execution while preserving review/revision and stage history |
| `src/provider.rs` | Remaining-budget timeout, call-boundary heartbeat integration, safe error redaction |
| `src/web.rs`, `src/main.rs` | Shared execution/contract/recovery API and CLI; preserve bounded legacy routes |
| `src/lib.rs`, `Cargo.toml`, `Cargo.lock` | Modules, chrono/MD5 compatibility dependencies, remove the unused `report_contract` declaration |
| `migrations/003_execution.sql` (replace draft) | Contract, scope, session, checkpoint, receipt, artifact, and readback ledgers/constraints |
| `web/app.js`, `web/index.html` | Shared execution route, contract selection, system/business states, gates, resume information |
| `tests/execution_contract.rs`, `tests/migration.rs` (new) | Contracts, continuous execution, injected failures, recovery, historical compatibility |
| `tests/reliability.rs`, `tests/agents.rs`, `tests/http.rs` | Stale writes, provider budget, background execution, new fields and legacy regression |
| `README.md`, `docs/architecture.md` | Actual behavior, legacy semantics, recovery, and limits |

### Contract and scope

Execute only supported JSON policies with pinned version, profile (`research`, `model_reviewed_research`, `weekly_report`), runtime, batch size, and SQL retries. Approved defaults: 900 seconds / 20 tasks / 2 retries. Allowed ranges: runtime 1..3600 seconds, batch 1..100, retries 0..2.

The registry is append-only. Verify SHA-256 integrity and required MD5 compatibility; digests do not authenticate authorship. Bind only an unstarted run after registration and separate readback. Create the run, sources/questions, and default binding atomically; never silently rebind defaults during startup or conceal a mismatch.

Scope is run ID plus its first execution mode. Reject mode/contract changes after work starts; local execution cannot bypass required model review. Weekly-report proof requirements remain explicit unsupported gates without fabricating data or exposing a send operation.

### Shared Runner and clock

`execute(run_id, mode, trigger, clock)` is the common entry point. The clock boundary supplies UTC time, monotonic elapsed time, and waiting; tests use a fake clock. Calculate KST explicitly.

1. Validate contract, mode, connection, and scope before claims. Rejected starts produce diagnostics only.
2. Observe expired sessions, reject a live duplicate, and persist before-counts and a session lease.
3. Recover missing receipts for committed results, then prioritize retried/expired tasks over new tasks.
4. Process one claim with a local or Researcher/Verifier executor; connect its evidence/answer/review to atomic persistence.
5. Commit, SELECT readback, store receipt, and record observed after-counts/last successful stage. Continue across batches.
6. On zero claims, requery state. Wait with heartbeats for work eligible within budget while handling other eligible tasks first.
7. Materialize and read back a Markdown artifact tied to current inputs; evaluate required SQL gates and record system/business states separately.
8. On a concrete blocker, limit, or failure, verify persistence and preserve PARTIAL diagnostics or safe recovery JSON.

Measure budget monotonically and leases in stored UTC. Initial session/task lease: 300 seconds; heartbeat target: every 30 seconds and at model-call boundaries. Bound requests by `min(existing 30 seconds, remaining budget)`. Expired tokens cannot heartbeat, write results, or finalize sessions. Record the actual timestamp with `next_eligible_after_budget` when the next eligible work falls outside budget.

### Data, transactions, and idempotency

| Data | Design |
| --- | --- |
| Contracts, bindings, execution scope | Append-only contract bodies, binding changes only before first claim, fixed run/mode fingerprint |
| Sessions | ID, run/mode/trigger, UTC start/end, KST start/end dates, lease, elapsed duration, state/stage, measured counts, error/resume |
| Batches | Task/delivery identity, result/source references, stable payload, hashes/count; same commit as results and queue transition |
| Readback receipts | Hash/count/state calculated by actual SELECTs and verification time, not an asserted success flag |
| Artifacts/readbacks | Bind body hash and input fingerprint; uniqueness covers run + input fingerprint + body hash |

Fingerprint contract references, execution scope, actual source hashes, questions/results, and model-review evidence in stable order. Equal output text cannot make an artifact valid for different inputs.

Read `user_version` within a transaction and apply only pending migrations. Preserve M1/M2 data and do not invent historical receipts. Diagnose legacy proof gaps; a new readback must not claim historical verification.

Retry only demonstrably safe operations. Generate operation/idempotency IDs outside retry loops. Bound busy/locked backoff to two retries. After a commit or readback-response failure, locate the existing commit and repair receipts instead of rerunning the task. Do not replay integrity rejection, stale-token writes, or constraint violations.

### Completion, diagnostics, and recovery

System states: running/completed/failed/interrupted. Business states: IN_PROGRESS/PARTIAL/COMPLETE. Gates cover pinned contracts, scope/mode, required tasks, evidence, storage integrity/readbacks, current artifacts, required model review, and external proof. Distinguish inapplicable gates from unavailable integrations.

Zero claims, batch boundaries, or LLM approval do not establish completion. Preserve legacy `status`; the UI uses new execution data for business completion. Stop codes describe observations: completed required work, evidence gaps, exhausted retries/budget, next eligibility after budget, unavailable provider, contract mismatch, storage/integrity failure, or expired session lease.

Preserve every AC-8 diagnostic field. Keep SQL and bindings separate. Export only allowlisted fields and redact keys, Authorization, and private settings. Atomically create a new recovery JSON beside the DB under `recovery/`, within the default Git-ignore boundary. If the file write also fails, return the failure and sanitized diagnostics rather than asserting storage.

### Interfaces and compatibility

| Interface | Behavior |
| --- | --- |
| `POST /api/contracts` | Register and read back supported version/body/digest |
| `POST /api/runs/{id}/contract` | Validate a pre-execution binding |
| `POST /api/runs/{id}/execute` | Accept mode/trigger and start the background Runner; acceptance is not completion |
| `GET /api/runs/{id}` | Add execution state, business state, counts, gates, and resume data without removing existing fields |
| `GET /api/recovery` | Scoped unfinished-run/session diagnostics; never implicitly execute another run |
| `GET /api/runs/{id}/recovery` | Sanitized recovery information/JSON |
| CLI `execute`, `contracts import`, `contract bind`, `recovery` | Same validation, Runner, and output model as HTTP |

Keep `/work`, `/agents`, and CLI `work` as bounded low-level compatibility routes with contract/scope guards. Their termination alone does not grant business COMPLETE. Dashboard Create/Resume/Retry uses `/execute`. Scheduled triggers share the Runner contract without installing cron or another automation.

### Draft reconciliation

Reuse policy/hash/KST ideas after validation, and complete append-only ledger constraints. Make creation/binding atomic; remove implicit startup default binding. Replace receipt-count-only completion with actual payload/current-input/proof checks. Include input fingerprints in artifact identity. Remove unused `report_contract`. Implement the shared Runner separately from legacy bounded wrappers.

### AC mapping and test strategy

| AC | Design | Main verification |
| --- | --- | --- |
| AC-1 | Registry/binding/preflight | Mismatch yields zero claims/model calls |
| AC-2 | Runner/batch continuation | One session processes and reads back 3 tasks at batch size 1 |
| AC-3 | Scope/recovery priority/fencing | Recovery order, other-run isolation, concurrent stale writes |
| AC-4 | Atomic batch/append-only/readback | Rollback, tampering, rejected UPDATE/DELETE, matching hashes/counts/state |
| AC-5 | Wait classification/fake clock | No false completion, other eligible work, delayed continuation |
| AC-6 | Completion/current artifact | System completed with business PARTIAL, model gate, input changes |
| AC-7 | Safe SQL retries/commit lookup | Two retries, non-transient rejection, post-commit interruption |
| AC-8 | Durable diagnostics/recovery JSON | Measured counts, stage, SQL/bindings, DB/file failure, redaction |
| AC-9 | Monotonic budget/UTC leases | Boundaries, actual elapsed time, valid stop codes, unknown interruption cause |
| AC-10 | Shared triggers/DB recovery | Equivalent triggers, restart, missing credentials |
| AC-11 | Versioned migrations/legacy | Preserve v1/v2, no invented receipts, reject stale artifacts |
| AC-12 | External-proof gates/safe UI/API | No external calls, fixture limits, no secret leakage, keyboard/text access |

Use real SQLite, injected failures, fake clocks, and mock providers. Run baseline regressions, fmt, and Clippy, then fresh read-only QA for every AC. Screenshots do not establish database or delivery proof.

### Proposed task decomposition

The design checkpoint approves this proposal. The live checklist is added to issue #2 after approval.

| Task | Work | AC |
| --- | --- | --- |
| T1 | Registry, typed validation, atomic run binding, fixed scope | AC-1, AC-3, AC-12 |
| T2 | Versioned migrations, ledger/receipt constraints, legacy diagnostics | AC-4, AC-11 |
| T3 | Atomic checkpoints, separate readback, input/artifact fingerprints | AC-4, AC-6, AC-7, AC-11 |
| T4 | Sessions, clock, heartbeat, continuous loop, waiting/budget/triggers | AC-2, AC-3, AC-5, AC-9, AC-10 |
| T5 | Local/model executors, timeouts, safe SQL retries | AC-1, AC-3, AC-7, AC-9 |
| T6 | Business gates, partial outcomes, safe DB/file recovery diagnostics | AC-6, AC-8, AC-10, AC-12 |
| T7 | API/CLI and legacy compatibility | AC-1, AC-6, AC-8, AC-10, AC-11 |
| T8 | UI contracts, system/business state, gates, resume/accessibility | AC-6, AC-8, AC-12 |
| T9 | Failure/time/restart/concurrency/compatibility tests and docs | AC-1–AC-12 |
| T10 | Independent QA preparation and Draft PR evidence (verification runs in test phase) | AC-1–AC-12 |

### Risks and alternatives

Use a new execution route instead of changing bounded legacy semantics; test that the UI actually migrates. Enforce single-host SQLite session ownership in SQL rather than relying on in-memory jobs. Restrict SQL retries to safe operations to avoid post-commit duplication. Test budgets and leases during model calls. Validate constraints and migrations against v1/v2 fixtures and copies, never production databases. Limit new dependencies to chrono and legacy-compatible MD5; retain sha2. Do not introduce a general workflow framework, queue service, or HTML parser.

## Verification

- **verdict:** FAIL — final CI reopened an AC-6 snapshot race; previous independent results are retained, with a focused re-test pending.
- **Tester:** `/root/independent_qa` (fresh read-only QA; incremental re-test by the same independent tester).
- **Reviewed range:** `origin/main..0dd4b84`, then fix `0dd4b84..aaeb482`.
- **Final local gate:** 41 tests passed (provider 2, agents 5, execution 15, HTTP/CLI 5, migration 3, reliability 11); fmt, Clippy, build and naming passed. HTTP tests also passed after the owned-port harness correction.

### Implementation gate evidence (2026-11-28)

39 local tests passed: provider 2, agents 5, execution contract 14, HTTP/CLI 5, migration 2, reliability 11. `scripts/dev.ps1 fmt`, `clippy`, `build`, and `scripts/check-conventions.ps1` passed. The full-suite invocation was `scripts/dev.ps1 test`; HTTP/model tests used local mocks, not production adapters.

Browser verification exercised contract selection, shared execution, textual states, recovery candidates and parameterized resume SQL. A three-question fixture produced system `completed`, business `PARTIAL`, three readbacks and an evidence gap. A fully supported one-question run produced system `completed`, business `COMPLETE`, one readback and no gates. A separate JavaScript failure injection confirmed a rejected contract binding never starts execution.

### Independent QA round 1

- **Tester:** `/root/independent_qa` (fresh read-only agent).
- **Reviewed head:** `0dd4b84`.
- **verdict:** FAIL (AC-4, AC-8); AC-1–3, AC-5–7, AC-9–12 passed the independent cases.
- Direct findings UPDATE was accepted; SQLite REPLACE could bypass DELETE guards with recursive triggers off; a committed checkpoint followed by a receipt failure reported the task-processing stage instead of checkpoint readback.
- Returned to implement; T2, T6 and T9 reopened. Fix scope: v4 immutable-ledger collision/observation guards, v3 upgrade preservation, safe idempotent insert callsites and typed post-commit stage diagnostics. Windows CI additionally exposed a wall-clock-dependent one-second budget test, replaced with an injected clock.

Independent re-verification is required for this incremental fix. Implementation and mock evidence do not establish production completion.

### Independent coverage (round 1, retained for unchanged behavior)

| AC | Performed boundary / negative cases and observed result | Result |
| --- | --- | --- |
| AC-1 | Seven minimum/maximum/out-of-range limit combinations and two unsupported policies; invalid policies rejected, reviewed-policy local start rejected with zero attempts. Digest/mode gate tests inspected. | PASS |
| AC-2 | Three supported questions, batch one: one session, three successes/readbacks, COMPLETE. Invalid/empty specs covered by the main gate. | PASS |
| AC-3 | Retried task before fresh task; other run stayed at zero attempts; active duplicate rejected before claims. Concurrent/stale-token gate evidence inspected. | PASS |
| AC-4 | Injected checkpoint failure rolled back findings and task transition; tampering blocked COMPLETE. Findings UPDATE and immutable-row REPLACE were accepted. | FAIL; fix requires re-test |
| AC-5 | Delayed retry completed after approximately 2.2 seconds; next eligibility beyond a one-second budget returned PARTIAL with its timestamp. | PASS |
| AC-6 | Evidence gap produced system completed/business PARTIAL with legacy partial; changed objective invalidated artifact/finalization. Actual model-proof gate inspected. | PASS |
| AC-7 | Post-commit receipt interruption repaired in a fresh invocation, attempts remained one and exactly one receipt. Bounded transient/semantic retry gate evidence inspected. | PASS |
| AC-8 | Injected Authorization error redacted; counts and bound SQL present. DB failure exported discoverable JSON with unknown counts null; DB plus file failure reported both. Post-commit stage was incorrect. | FAIL; fix requires re-test |
| AC-9 | Maximum limits accepted and invalid limits rejected; budget boundary preserved eligibility; expired session recorded interrupted/session_lease_expired with unknown cause. Monotonic exhaustion gate inspected. | PASS |
| AC-10 | Three CLI triggers gave equivalent gap outcomes across fresh processes; missing provider left attempts zero. Restored agent UI requested reconnect and made zero execute calls. | PASS |
| AC-11 | v1/v2 historical findings and v2 answer/verifier event retained with original timestamps, zero invented storage/model receipts; objective changes invalidated stale artifacts. | PASS |
| AC-12 | Weekly fixture and bounded-route bypass remained blocked by external proof gates. Auth error redacted. Plain-text rendering, labeled native selector, buttons/details/summary and alert semantics inspected. | PASS |

### Dimensions exercised

| Dimension | Disposition and evidence |
| --- | --- |
| Functional / boundaries / errors | Applied across AC-1–12 through independent CLI/SQLite and JavaScript VM probes. |
| State / data integrity | Applied: atomic rollback, receipt interruption, mutations, replacement collision, current-input artifacts and history. |
| Concurrency / recovery | Applied: active duplicate, scope/priority probes; main-gate stale/concurrent claim results reviewed. |
| API / schema / compatibility | Applied: actual migrations plus legacy CLI and HTTP route dependency review. |
| Privacy / accessibility | Applied within local scope: structured policies, redaction/allowlist, plain-text rendering and native keyboard controls. No screen-reader certification asserted. |
| Regression | **Applied.** Full changed definitions and callers checked: Engine create/get/list/claim/work/finish/report; contracts; checkpoints/artifacts; Runner; agent preflight/background processing; provider; HTTP execute/work/agents/report/recovery; CLI execute/work/status/list/report/demo; dashboard create/bind/process/poll/retry/resume. |
| Production / multi-host / scheduler | Skipped: explicitly outside scope; fixtures prove no external delivery. |
| Performance benchmarks | Skipped: no throughput/latency AC; execution-budget boundaries were exercised. |

Independent evidence was generated in ignored local scratch: seventeen probe groups, five supplemental migration/fallback/preflight probes, UI reconnect checks, replacement and stage reproductions. Private local data and recovery files are not published. Round 1 reused the performed 39-test main gate instead of rerunning it.

### Independent round 2 — incremental fixes

| Criteria | Expected and observed result | Result |
| --- | --- | --- |
| AC-1, AC-4 | Findings UPDATE/DELETE rejected. All 14 immutable collision identities rejected with recursive triggers off, including alternate unique keys and frozen binding. | PASS |
| AC-1, AC-3 | Scope movement, post-start rebinding and conflicting execution mode rejected; stored binding/mode unchanged. | PASS |
| AC-4, AC-7 | Three repeated report/recovery invocations retained COMPLETE, sources/batches/artifacts 1/1/1, attempts 1. | PASS |
| AC-7, AC-8 | Post-commit receipt failure recorded `checkpoint_commit` then `checkpoint_readback`, succeeded 1/readbacks 0. Recovery reached COMPLETE/readback 1 with attempts still 1. Pre-commit failure correctly retained task_processing/succeeded 0. | PASS |
| AC-11 | All rows in 14 v3 tables unchanged after v4 upgrade; mutation guards installed; repeat open preserved data. | PASS |
| AC-9, regression | Virtual clock advances only on requested sleep. Four independently launched concurrent port-zero HTTP servers had distinct owned ports and isolated run identities. | PASS |

Regression dimension remains applied to migration, persistence, receipts/artifacts, typed error propagation, Runner, budget tests and HTTP startup callers. Unchanged AC cases retain round 1 PASS. The independent tester reused the performed 41-test main gate instead of repeating the full suite. Parent browser regression additionally verified persisted COMPLETE after restart and Enter/Tab/Escape API-dialog control. This remains supplied-document completion only; no production adapter/delivery proof is asserted.

### Final CI snapshot regression

CI run `46047562814` observed an inactive job flag paired with a previously read running SQL snapshot. Returned to implement (T7/T9, AC-6). The detail handler now holds the job-state mutex only inside the synchronous database callback while reading and serializing the run plus flag. Reservation and job termination cannot interleave that read; no mutex spans an await. The existing completion-boundary HTTP case now repeats eight runs and retains the strict terminal-state assertion. The 41-test local gate, fmt, Clippy and build passed after this correction; focused independent verification is pending.
