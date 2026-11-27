# ResearchOps agent instructions

Read [the project SDLC router](docs/sdlc/router.md) before changing project or feature state. Use the installed `sdlc-agent-kit` procedures with the confirmed GitHub mapping recorded there. This repository is not a GitLab project; do not run the kit's GitLab installer or replace the GitHub remote.

## Established project decisions

- Implementation language: Rust. Current runtime: local Axum dashboard/API, CLI, SQLite, optional OpenAI-compatible model adapter.
- Base branch: `main`; changes go through PRs. Preserve existing work and published history.
- Branches: `[codex/]<type>/<kebab-case-description>`; PR titles and new commit subjects use Conventional Commit-style naming. See [CONTRIBUTING.md](CONTRIBUTING.md).
- Required GitHub approving reviews: zero. The naming check is required; deletion and force pushes to the default branch are blocked. Never self-approve a PR or change these policies without a user request.
- Canonical project and feature documents live in this repository under `docs/project/` and `docs/features/`. Tracking/checklists live in GitHub Issues and PRs. Do not duplicate live progress in project documents.
- Do not publish raw user attachments, API keys, private production identifiers, recipient addresses, or local recovery data. Translate operational contracts into requirements and reference private integration inputs without copying them into the public repository.
- Contract bodies, template hashes, production verification, and email delivery are facts only when observed through the relevant adapter. Missing adapters or credentials must remain explicit blockers; fixtures cannot prove production completion.

## Validation

Use `cargo fmt -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, and tests appropriate to the approved feature. On this Windows workspace, `scripts/dev.ps1` selects the ignored project-local toolchain when present. Run `scripts/check-conventions.ps1` for naming validation.

Independent feature verification must use a fresh read-only QA agent as required by `sdlc-agent-kit:feature-test`. Implementation work is not verified merely because the same agent wrote and tested it.
