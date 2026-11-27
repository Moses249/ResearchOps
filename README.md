# ResearchOps

ResearchOps helps researchers organize research questions, gather supporting evidence from source materials, and produce reports with findings traceable to their original sources.

This Rust MVP implements that workflow with durable tasks: define questions, supply document snapshots, process resumable work, inspect matching passages, and export a Markdown evidence report.

**MVP v0.1:** local keyword mode needs no API key, Docker, or external database. Optional API mode connects Researcher and Verifier agents through an OpenAI-compatible Chat Completions endpoint. The dashboard and API bind to `127.0.0.1` only.

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
- Session-only API connections, two-model connection test, and background agent runs.
- Researcher → exact-quote gate → Verifier → feedback/revision, with at most three rounds per delivery.
- Persistent agent activity, model-reviewed answers, and explicit failed-task retry within a three-delivery limit.

## API connection and agent mode

1. Open **Connect API** in the dashboard.
2. Enter a base URL such as `https://your-provider.example/v1`, the Researcher model ID, the Verifier model ID, and an API key. Local endpoints can use `http://127.0.0.1:PORT/v1` without a key.
3. Select **Save connection**, then **Test both models**. Testing makes two small model requests and verifies JSON output support.
4. Enable **Use Researcher → Verifier agents** before creating a new run. The supplied documents are sent to the configured provider. API mode is limited to 120 KB of source text per run.
5. Inspect **Agent activity** for proposals, rejection feedback, revisions, and final outcomes. Failed tasks can be requeued using **Retry failed tasks** while their delivery count is below three.

The API must support `POST /chat/completions` with `response_format: {"type":"json_object"}`. Native Anthropic/Responses endpoints are not supported by this adapter. Researcher and Verifier may use different model IDs, or the same model in separate conversations. Separate conversations do not guarantee independent judgments.

API keys are held only in server memory, never returned by the API or written to SQLite, browser storage, logs, or Git. Saving replaces the connection; re-enter the key when editing. Restarting the server clears it. Disconnecting prevents new agent jobs; already running jobs retain their connection until completion. Remote endpoints require HTTPS; HTTP is allowed only on loopback. Redirects are refused, requests time out after 30 seconds, and error responses are not echoed.

Agent jobs run in the background while the browser polls progress. Each task delivery has a five-minute lease and at most three research/review rounds with short backoff. Three rejected rounds fail the task without committing an answer or findings. Manual retries keep history and consume the remaining delivery budget; crashes are recovered after lease expiry. A delivery can make up to six model requests, so a task's three-delivery lifetime can make up to eighteen requests. There is no automatic supervisor after server restart: reconnect and resume the run.

## Interpret results carefully

This release works over supplied source snapshots. It does not browse the web, fetch source URLs, establish factual truth, or independently audit omissions. Local mode extracts keyword matches; optional API mode proposes and reviews answers grounded in those snapshots.

`exact_quote` means the excerpt is present in the stored snapshot. Keyword overlap is a retrieval score, not a probability or confidence estimate. `complete` means every question task succeeded and has evidence. In API mode, answers additionally passed a separate model review. Neither status proves correctness or exhaustive source coverage.

Search uses lowercased alphanumeric tokens with exact token matching. It is not a stemmer, semantic search engine, or Korean morphological analyzer. For Korean, reuse exact terms appearing in the documents. Sentence splitting is intentionally simple; passages longer than 1,200 characters are skipped. Input is limited to 100 questions, 100 documents, 1 MB per document, and 5 MB total source text.

Inputs are immutable after run creation. Create a new run to change questions or snapshots. Use the dashboard's **Process / resume** button or the CLI `work` command. Local-mode leases default to 60 seconds; attempts are capped at three. A work call processes at most 100 tasks by default. CLI `work` uses local keyword mode; model agents are started through the dashboard or HTTP API.

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
| GET / POST / DELETE | `/api/connection` | View redacted session config, replace it, or disconnect |
| POST | `/api/connection/test` | Test both configured models; body `{}` |
| POST | `/api/runs/{id}/agents` | Start a background agent job; body `{ "limit": 100 }` |
| POST | `/api/runs/{id}/retry` | Requeue failed tasks with remaining deliveries; body `{}` |

Mutations require JSON. Cross-origin browser access is not enabled. The API never follows source URLs and exposes no endpoint for reading arbitrary local files.

## Reliability design

SQLite runs in WAL mode with foreign keys and a five-second busy timeout. A short `BEGIN IMMEDIATE` transaction claims a task and assigns a fresh lease token. Extraction happens outside that transaction. The completion transaction checks both the token and the unexpired lease before inserting findings and marking the task successful.

If a worker crashes, its task becomes claimable after expiry. A late worker cannot commit after expiry or reassignment. A failed evidence check rolls back both findings and task completion. Findings have a unique `(task, source, quote)` key. This is **at-least-once task delivery with idempotent persistence**, not exactly-once execution.

Model API calls are external side effects and may be repeated after a crash; provider billing is not exactly-once. Answers and evidence are committed atomically after verification, while the event transcript remains available on failure. SQLite is the single-machine MVP backend; distributed workers and PostgreSQL are future work.

See [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md).

## Verification

```sh
cargo test --locked
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
```

Integration tests cover restart persistence, a known coverage gap, worker concurrency, lease recovery, stale writes, retry exhaustion, evidence tampering, duplicate inputs, input validation, and Unicode/markup handling. CI runs these checks on Linux and Windows.

Mock-provider tests cover fabricated quotes, rejection feedback, successful revision, bounded failure, and redacted API connections without making paid API calls. Live provider compatibility requires supplying and testing your own connection.

See [CONTRIBUTING.md](CONTRIBUTING.md) for enforced PR title, commit message, and branch naming conventions.

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
