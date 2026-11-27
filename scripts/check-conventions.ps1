param(
    [string]$BaseRef = 'origin/main',
    [string]$HeadRef = 'HEAD',
    [string]$BranchName = '',
    [string]$PrTitle = '',
    [switch]$SelfTest
)
$ErrorActionPreference = 'Stop'
$types = 'feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert'
$titlePattern = "^($types)(\([A-Za-z0-9._/-]+\))?!?: .+"
$branchPattern = "^(main|develop|(codex/)?($types)/[a-z0-9]+([/-][a-z0-9]+)*)$"
if ($SelfTest) {
    foreach ($sample in @('feat: add API connections', 'fix(engine): reject stale writes', 'chore!: change configuration')) {
        if ($sample -cnotmatch $titlePattern) { throw "Valid title rejected: $sample" }
    }
    foreach ($sample in @('add feature', 'Feat: wrong case', 'fix:')) {
        if ($sample -cmatch $titlePattern) { throw "Invalid title accepted: $sample" }
    }
    foreach ($sample in @('codex/feat/api-agent-workflow', 'fix/stale-lease', 'main')) {
        if ($sample -cnotmatch $branchPattern) { throw "Valid branch rejected: $sample" }
    }
    foreach ($sample in @('codex/rust-mvp', 'Feature/test', 'feat/mixed_CASE')) {
        if ($sample -cmatch $branchPattern) { throw "Invalid branch accepted: $sample" }
    }
    Write-Output 'Naming convention self-tests passed.'
    exit 0
}
if (-not $BranchName) { $BranchName = git branch --show-current }
if ($BranchName -cnotmatch $branchPattern) { throw "Branch must use [codex/]<type>/<kebab-case-description>: $BranchName" }
if ($PrTitle -and $PrTitle -cnotmatch $titlePattern) { throw "PR title must use <type>[optional scope]: description" }
# Historical published commits are grandfathered; never rewrite their history.
$legacy = @('76849d92d99721b14a8b38ef06be81545f5a0758', '2a78edd0672bbfff5224caae7b8d5eac3a2e1b99')
$commits = @(git rev-list --no-merges "$BaseRef..$HeadRef")
if ($LASTEXITCODE -ne 0) { throw 'Cannot determine commit range.' }
foreach ($commit in $commits) {
    if ($legacy -contains $commit) { continue }
    $subject = git show -s --format=%s $commit
    if ($LASTEXITCODE -ne 0) { throw "Cannot read commit $commit" }
    if ($subject -cnotmatch $titlePattern) { throw "Non-conventional commit $commit : $subject" }
}
Write-Output "Naming conventions passed for $BranchName."
