param(
    [ValidateSet('serve', 'demo', 'test', 'fmt', 'format', 'clippy', 'build')]
    [string]$Task = 'serve'
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    $localCargo = Join-Path $projectRoot '.tools\cargo\bin\cargo.exe'
    if (Test-Path -LiteralPath $localCargo) {
        $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
        $env:PATH = "$(Join-Path $projectRoot '.tools\cargo\bin');$env:PATH"
        $localMingw = Join-Path $projectRoot '.tools\w64devkit\bin'
        if (Test-Path -LiteralPath $localMingw) {
            $env:PATH = "$localMingw;$env:PATH"
            # Use the matching MinGW runtime libraries shipped with this Rust toolchain.
            if ($env:RUSTFLAGS -notmatch 'link-self-contained') {
                $env:RUSTFLAGS = "$env:RUSTFLAGS -C link-self-contained=yes".Trim()
            }
        }
        $cargoCommand = $localCargo
    } else { $cargoCommand = 'cargo' }
    [string[]]$CargoArgs = @(switch ($Task) {
        'serve' { @('run', '--locked', '--', 'serve') }
        'demo' { @('run', '--locked', '--', 'demo') }
        'test' { @('test', '--locked') }
        'fmt' { @('fmt', '--', '--check') }
        'format' { @('fmt') }
        'clippy' { @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings') }
        'build' { @('build', '--locked') }
    })
    & $cargoCommand @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "Cargo exited with code $LASTEXITCODE" }
} finally { Pop-Location }
