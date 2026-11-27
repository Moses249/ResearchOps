# ResearchOps

ResearchOps helps researchers organize research questions, gather supporting evidence from source materials, and produce reports with findings traceable to their original sources.

This Rust MVP implements that workflow with durable tasks: define questions, supply document snapshots, process resumable work, inspect matching passages, and export a Markdown evidence report.

**MVP v0.1:** runs locally, without an LLM, API key, Docker, or external database. The dashboard and API bind to `127.0.0.1` only.

## Quick start

With a stable Rust toolchain and a working native C toolchain:

```sh
cargo run --locked -- serve
```

Open <http://127.0.0.1:8787>. Select **Load example**, then **Create & run research**. The synthetic example deliberately leaves the benchmark question uncovered: expect **partial**, with two of three questions having lexical evidence.

On this Windows workspace, a project-local Rust toolchain and C compiler have been installed under the ignored `.tools` directory. Use:

```powershell
.\scripts\dev.ps1
```

The helper uses the local toolchain when present and otherwise uses `cargo` from PATH. Tool downloads are not committed and the helper does not install software on a fresh checkout. Windows MSVC users need Visual Studio C++ Build Tools; GNU users need a compatible MinGW compiler. SQLite is compiled from the bundled source.

## What works

- Dashboard to create research requests, add document snapshots, inspect saved runs, resume processing, and export reports.
- CLI and JSON HTTP API over the same Rust engine.
- Durable SQLite task state with atomic claims, expiring leases, fencing tokens, three attempts, and exponential retry delay.
- One task per unique question; duplicate source content is stored once per run.
- Deterministic keyword retrieval: the best matching passage from each supplied document.
- Exact-quote verification against an immutable source snapshot and its SHA-256 hash.
- Per-question coverage gaps and explicit pending/running/complete/partial states.
- Durable results: restarting the application retains runs, attempts, and evidence.

## Interpret results carefully

This release is an **extractive evidence workspace**, not an autonomous deep-research agent. It does not browse the web, fetch source URLs, use an LLM, validate factual truth, detect contradictions, or independently audit omissions.

`exact_quote` means the excerpt is present in the stored snapshot. Keyword overlap is a retrieval score, not a probability or confidence estimate. `complete` means every question task succeeded and has at least one lexical match. It does **not** mean every question is answered correctly or all relevant sources have been found.

Search uses lowercased alphanumeric tokens with exact token matching. It is not a stemmer, semantic search engine, or Korean morphological analyzer. For Korean, reuse exact terms appearing in the documents. Sentence splitting is intentionally simple; passages longer than 1,200 characters are skipped. Input is limited to 100 questions, 100 documents, 1 MB per document, and 5 MB total source text.

Inputs are immutable after run creation. Create a new run to change questions or snapshots. There is no automatic worker daemon: use the dashboard's **Process / resume** button or the CLI `work` command. Delayed retries and expired leases are recovered on the next work invocation. Leases default to 60 seconds; attempts are capped at three. A work call processes at most 100 tasks by default.

The database is unencrypted. Keep sensitive documents on a trusted machine. This local-only MVP has no multi-user authentication and is not intended for public hosting.

## CLI

```sh
# Bundled synthetic example, including an intentional evidence gap
cargo run --locked -- demo

# Create returns the run UUID
cargo run --locked -- create --spec examples/research.json
cargo run --locked -- work --run RUN_UUID --limit 100 --lease-seconds 60
cargo run --locked -- status --run RUN_UUID
cargo run --locked -- list
cargo run --locked -- report --run RUN_UUID --output report.md

# Separate database or port
cargo run --locked -- --db data/demo.db serve --port 8788
```

PowerShell helper equivalents:

```powershell
.\scripts\dev.ps1 demo
.\scripts\dev.ps1 test
.\scripts\dev.ps1 fmt
.\scripts\dev.ps1 clippy
```

The specification format is demonstrated in [examples/research.json](examples/research.json). Each source has `title`, optional `uri`, and `content`. A URI is a stored reference, never a network request or file read.

## HTTP API

| Method | Route | Behavior |
| --- | --- | --- |
| GET | `/health` | Server liveness |
| GET | `/api/example` | Synthetic sample specification |
| GET | `/api/runs` | Latest 100 runs and evidence |
| POST | `/api/runs` | Create from a research specification; returns `{ "id": "..." }` |
| GET | `/api/runs/{id}` | Run status, questions, attempts, findings |
| POST | `/api/runs/{id}/work` | Process tasks; JSON body `{ "limit": 100 }` |
| GET | `/api/runs/{id}/report` | Download Markdown evidence report |

Mutations require JSON. Cross-origin browser access is not enabled. The API never follows source URLs and exposes no endpoint for reading arbitrary local files.

## Reliability design

SQLite runs in WAL mode with foreign keys and a five-second busy timeout. A short `BEGIN IMMEDIATE` transaction claims a task and assigns a fresh lease token. Extraction happens outside that transaction. The completion transaction checks both the token and the unexpired lease before inserting findings and marking the task successful.

If a worker crashes, its task becomes claimable after expiry. A late worker cannot commit after expiry or reassignment. A failed evidence check rolls back both findings and task completion. Findings have a unique `(task, source, quote)` key. This is **at-least-once task delivery with idempotent persistence**, not exactly-once execution.

No network side effects are present in this release. Future outbound adapters will need their own idempotency and timeout rules. SQLite is the single-machine MVP backend; distributed workers and PostgreSQL are future work.

See [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md).

## Verification

```sh
cargo test --locked
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
```

Integration tests cover restart persistence, a known coverage gap, worker concurrency, lease recovery, stale writes, retry exhaustion, evidence tampering, duplicate inputs, input validation, and Unicode/markup handling. CI runs these checks on Linux and Windows.

## Repository

```text
src/engine.rs              Durable queue, evidence extraction, report generation
src/web.rs                 Local HTTP API and embedded dashboard
src/main.rs                CLI
migrations/001_initial.sql SQLite schema
web/index.html             Dependency-free dashboard
examples/research.json     Synthetic reproducible demo
tests/reliability.rs       Integration tests
scripts/dev.ps1           Windows development helper
```

Licensed under MIT.
