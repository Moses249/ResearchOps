> **status:** confirmed · 2026-11-27 · owner @Moses249

# Project Plan: ResearchOps

English translation of the [approved Korean plan](Plan.md). This translation preserves the approved scope and decisions; it does not establish a separate approval or implementation status.

## Summary

ResearchOps is a Rust-based research operations tool that breaks research objectives into executable tasks, collects and verifies evidence, and produces reports traceable to their sources. Its central value is preserving enough state to resume interrupted or rejected work while making unfinished scope visible.

Job intelligence is the first use case. The same execution engine should support literature reviews, technology comparisons, and market research. Live scraping is a replaceable source adapter.

## Target users / personas

| User | Context | Core need |
| --- | --- | --- |
| Individual researcher or job seeker | Repeatedly compares documents, job listings, and other sources | Results with visible evidence and unresolved conditions; continuation of previous work |
| Research workflow operator | Manages recurring or long-running tasks and report delivery | Before/after counts, failure stages, exact resume positions, and duplicate-delivery prevention |
| Systems developer or verifier | Connects research agents and source adapters | Explicit contracts, observable execution state, and reproducible failure verification |

## Purpose / value

Prevent session termination or a model's success message from being mistaken for business completion. Judge outcomes against actual persistence, readback, and role-specific completion conditions so users can inspect unfinished work and the next action.

## Scope — product level

### In scope

| Capability | Product scope |
| --- | --- |
| Research objectives and boundaries | Explicit questions, sources, evaluation criteria, budgets, and completion conditions |
| Durable execution | SQL work queues, leases and fencing tokens, small batches, checkpoints, and unfinished-work-first recovery |
| Evidence and verification | Source snapshots, exact quotations, integrity checks, and feedback between Researcher and Verifier agents |
| Honest completion assessment | Separate system execution state from business completion; provide specific partial-result reasons |
| Results and delivery | Traceable reports, fixed-template and manifest validation, duplicate prevention, and delivery readback through adapters |
| Domain extension | Connect job policies, academic research rules, and source adapters to the shared engine |
| Operations and evaluation | Execution history, observed before/after counts, explicit resume positions, fault injection, and quality evaluation |

### Non-goals and boundaries

| Boundary | Reason |
| --- | --- |
| Guaranteeing completeness across the entire internet | Only execution and omissions within a defined scope can be assessed |
| Establishing facts, application eligibility, or delivery solely from model judgment | Source evidence, persistence, and actual delivery proof are required |
| Unconditional recovery after platform termination | Execution-environment constraints must be recorded; only observed recovery may be claimed |
| An initial general-purpose plugin marketplace or multi-user SaaS | First demonstrate execution, evidence, and recovery contracts for individual use |
| Immediately executing attached operational instructions against production | Implementation requirements must be distinguished from verified permissions, environment, and data |

## Primary scenarios

| # | Actor | Journey and expected result |
| --- | --- | --- |
| 1 | Individual researcher | Enter objective, questions, and documents → inspect source evidence → export a report |
| 2 | Individual researcher | Test API connection → research proposal → separate verification → rejection feedback and revision → evidence-linked result |
| 3 | Operator | Inspect unfinished work → prioritize recoverable tasks → persist and read back small batches → continue to the next stage |
| 4 | Operator | Encounter a concrete blocker or configured limit → preserve partial results → inspect actual errors, before/after counts, last successful stage, and resume position |
| 5 | Job intelligence operator | Discover, verify, audit, and assess within country/role boundaries → classify candidates using current evidence and eligibility conditions |
| 6 | Report operator | Validate current assessments, fixed template, and manifest extracted from the rendered output → check duplicates → deliver through an authorized route → read back the sent message and save a receipt |

## Feature boundaries for the initial cycle

The first feature is **execution-completion-contract**. It covers shared execution continuity, persistence readback, recovery, completion assessment, and explicit blocking when required external verification is unavailable.

Job-specific discovery paths, scoring and remote-eligibility policies, live Supabase integration, the fixed report renderer, and Gmail delivery are separate follow-up features with their own required inputs and approved acceptance criteria. Implementing the shared execution contract must not mark those domain workflows complete.

This document defines product direction. Detailed acceptance criteria, design, and task checklists belong in each feature document and GitHub issue.

## Confirmed assumptions

- Prioritize individual use and local execution, with public development on GitHub.
- Start from the Rust core and current SQLite MVP; extend external services through verifiable adapters.
- Decompose the supplied job-operations contract into shared execution infrastructure and job/report domain features.
- The first feature does not execute production discovery or send email. Missing integrations or evidence block completion conditions.

## Decision log

| Date | Decision |
| --- | --- |
| 2026-11-27 | Maintainer @Moses249 approved the GitHub SDLC mapping and adoption of the existing MVP baseline. Retain GitHub, main, naming conventions, and zero required approving reviews; introduce independent QA. |
| 2026-11-27 | Maintainer @Moses249 approved the product scope, scenarios, and initial feature boundaries by replying “ㄱㄱ” to the project-plan confirmation request. |
