# Contribution conventions

Use Conventional Commit-style subjects for PR titles and new commits:

```text
feat: add API connections
fix(agents): reject unsupported evidence
chore: configure repository ruleset
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`. An optional scope and `!` breaking-change marker are supported. Use a nonempty description.

Branches use `[codex/]<type>/<kebab-case-description>`, for example `codex/feat/api-agent-workflow` or `fix/stale-lease`. `main` and `develop` are integration branch exceptions.

Enable local hooks after cloning:

```sh
git config core.hooksPath .githooks
```

CI validates branch names, PR titles, and new non-merge commit subjects. The two commits published before this policy are explicitly grandfathered by SHA; their history is not rewritten. Merge commit subjects are excluded, while the PR title is validated. Prefer squash merging so the PR title becomes the final commit subject.

The repository ruleset protects the default branch from deletion and force pushes, requires PRs, and requires the **Naming conventions** check. Required approvals: **0**, for solo development. No bypass actors are configured.

Run the policy checker locally with PowerShell 7:

```powershell
.\scripts\check-conventions.ps1 -SelfTest
.\scripts\check-conventions.ps1 -PrTitle 'feat: add API connections and agent verification'
```
