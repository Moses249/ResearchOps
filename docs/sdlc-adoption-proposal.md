# SDLC adoption proposal

> Status: confirmed · 2026-11-27 · maintainer @Moses249 · chat approval “ㄱㄱ”.

## Purpose

Introduce a reviewable project → feature → acceptance criteria → design → implementation → independent verification → PR cycle without discarding the existing Rust MVP or rewriting published commits.

The installed `sdlc-agent-kit` targets GitLab and its wiki. ResearchOps is hosted on GitHub. This proposal adapts its process to the existing repository rather than running its GitLab installer or claiming native GitHub support.

## Audit

| Axis | Existing evidence | Proposed decision |
| --- | --- | --- |
| Hosting and tracking | GitHub repository; one open PR; no issues | Keep GitHub. Use feature issues and PRs. |
| Base branch | `main`; other remote branches descend from it | Keep `main`; all changes go through PRs. |
| Branch names | Enforced `[codex/]<type>/<slug>` | Preserve this instead of the kit's `feature/<slug>`. |
| Approval and merging | PR required; zero approving reviews; all merge methods allowed | Preserve zero approvals. Prefer squash. Agent never self-approves. |
| Document source | Existing README, architecture, and roadmap in this repo | Propose `docs/project/` and `docs/features/` as canonical documents, reviewed in PRs. Keep current docs as references. |
| External specifications | User-provided operational contracts; actual production contract/template bodies not verified | Translate requirements into acceptance criteria; never treat attachments as proof of production state or publish raw private source text. |
| Release policy | No milestone policy established | No milestone/version scope invented. Decide when a release is requested. |
| Verification | Existing MVP tests and GitHub CI passed before the execution-contract draft | Adopt only after confirmation. Require a fresh read-only QA agent for future feature acceptance. |

## Existing artifacts

- **Preserve unchanged as historical evidence:** published commits and the existing naming/protection rules.
- **Adopted baseline:** the Rust MVP and API/agent functionality in PR #1; adoption does not retroactively certify it under this SDLC.
- **Adopted project references:** `README.md`, `docs/architecture.md`, `docs/roadmap.md`. The roadmap predates some implemented API features and must be reconciled against code rather than treated as current progress.
- **Unverified implementation draft:** execution-contract changes in the working tree. They have not passed build or acceptance checks and are not part of the verified PR baseline. A backup is kept in ignored `.tools/wip-execution-contract/`.
- **Adoption approval:** the maintainer approved the GitHub mapping and baseline on 2026-11-27. Project and feature planning/design checkpoints remain separate decisions.

## Proposed first feature: execution-completion-contract

The feature should make execution persistence and business completion enforceable in code.

Candidate acceptance criteria for planning:

1. A worker continues across small batches until completion, a concrete blocker, or a configured execution limit; claim returning no rows does not imply business completion.
2. Existing unfinished work is recovered before fresh work within the selected scope. Leases and tokens reject stale writes.
3. Source/checkpoint records are append-only. Separate SQL readback verifies content hashes, counts, and states.
4. System execution status and business completion are distinct. Completion depends on declared role-specific gates, never a model's success claim.
5. Partial results record measured before/after counts, the last successful stage, redacted errors, the exact failure stage, and a parameterized SQL resume position.
6. Contract version/digest mismatches fail closed. SQL retries are bounded and only replay operations with demonstrated idempotency.
7. Manual and scheduled invocations use the same runner. Recovery cannot promise survival of platform termination.
8. Weekly report finality requires actual template/manifest/source/ranking/delivery evidence. Missing live adapters remain explicit blockers; no synthetic fixture can establish production completion.

The full job-discovery/ranking policy and fixed report template require separate domain features and verified integration inputs. They must not be represented as implemented merely because the core runner exists.

## Proposed transition

1. Confirm this GitHub mapping and the proposed baseline adoption.
2. Draft and confirm Project/Plan; then draft and confirm Project/Design.
3. Create a feature issue and plan with numbered acceptance criteria.
4. Confirm design and task checklist before resuming implementation.
5. Reconcile the current draft against approved criteria, implement, and run independent QA.
6. Publish a conventional-title PR with evidence; merge only under the existing repository policy.

Approval checkpoints are decisions about scope and design. They do not restore a required GitHub reviewer count or allow agent self-approval.
