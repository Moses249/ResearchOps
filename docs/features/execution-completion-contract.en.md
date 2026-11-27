> **phase:** planning

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

Write after Feature Plan approval. Do not finalize an implementation checklist before that checkpoint.

## Verification

Record independent AC-specific observations after approved implementation. Do not reuse baseline tests or unverified draft code as a PASS for this feature.
