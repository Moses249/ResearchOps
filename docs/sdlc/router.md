# ResearchOps SDLC router — GitHub adaptation

> Adoption confirmed: 2026-11-27 · maintainer @Moses249 · chat approval “ㄱㄱ”.

This is a project-specific adaptation of the installed `sdlc-agent-kit`, whose original tracker targets GitLab. Preserve its phase gates and independent QA requirement while using the GitHub mapping below. Existing user decisions override the kit's platform and branch defaults.

## Platform mapping

| Kit concept | ResearchOps source of truth |
| --- | --- |
| GitLab project | GitHub `Moses249/ResearchOps` |
| Project wiki Plan | `docs/project/Plan.md` |
| Project wiki Design | `docs/project/Design.md` |
| Feature wiki page | `docs/features/<slug>.md` |
| Feature issue and task checklist | One GitHub issue with `type::feature`, one `status::` label, and AC-tagged task checklist |
| Feature branch | `codex/feat/<slug>` or another established conventional type |
| Draft MR / MR | Draft GitHub PR / GitHub PR |
| Tracker operations | `gh` or an available GitHub connector; never GitLab `tracker.sh` against this remote |
| Wiki marker | The explicit document `status` / `phase` marker; never infer a phase from code or branch names |

## Orientation and gates

1. Read `docs/project/Plan.md` and `docs/project/Design.md`. If either is absent or draft, follow the installed `project-inception` procedure. Confirm Plan before authoring Design; confirm Design before deriving the initial feature backlog.
2. A feature has one document with `> **phase:** planning|design|implement|test|release`. Read that marker, then read the corresponding installed skill.
3. Feature Plan owns numbered `AC-n` acceptance criteria. Design maps them to components and fills the GitHub issue task checklist. Obtain explicit maintainer confirmation at both checkpoints before advancing.
4. Implementation uses one branch and one Draft PR per feature. Preserve unrelated work and inspect new issue/PR feedback at task boundaries.
5. Verification delegates every AC to a fresh read-only QA agent. Record observed evidence and limitations; defects return to implementation.
6. Release follows the repository's merge policy. Zero required approving reviews does not imply agent self-approval or permission to invent a milestone release.

Use `status::{backlog,in-progress,review,done,cancelled}` for tracking. Documents contain specifications, decision records, and verification evidence; Issues/PRs own current task completion and live progress.

## Confirmed baseline adoption

- The existing Rust MVP and API/agent work published in PR #1 are adopted as the implementation baseline, not retroactively asserted to have completed this SDLC.
- Existing README, architecture, and roadmap are adopted as supporting references. Reconcile their content with code before deriving remaining features.
- Published commits are preserved, including the naming policy's two explicit historical exceptions.
- The uncommitted execution-contract draft is reference implementation material. It must be reconciled against approved acceptance criteria and pass independent verification before release.

See [the adoption record](../sdlc-adoption-proposal.md) for the audit and decisions.

## Checkpoint record

Record explicit approval with date, maintainer, and the exact artifact/version approved. A previous approval is reusable for that same decision, not for unknown future scope. GitHub review-count settings and planning/design checkpoints serve different purposes.

## Release policy

No milestone or release version has been selected. Do not invent one. Default branch is `main`, preferred merge method is squash, and repository protection remains unchanged. The maintainer decides when to merge or release.
