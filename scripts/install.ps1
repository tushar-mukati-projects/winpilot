param(
    [switch]$SkipBuild,
    [switch]$Register
)

$ErrorActionPreference = "Stop"
$repo = Join-Path $PSScriptRoot ".."
$built = Join-Path $repo "target\release\deckhand.exe"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\deckhand"
$installed = Join-Path $installDir "deckhand.exe"
$retired = Join-Path $installDir "deckhand.old.exe"

if (-not $SkipBuild) {
    Push-Location $repo
    try {
        cargo build --release -p deckhand
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $built)) { throw "no release build at $built" }
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

if (Test-Path $retired) {
    Remove-Item $retired -ErrorAction SilentlyContinue
}
if (Test-Path $installed) {
    Move-Item $installed $retired
}

Copy-Item $built $installed
$expected = (Get-FileHash $built -Algorithm SHA256).Hash
$actual = (Get-FileHash $installed -Algorithm SHA256).Hash
if ($expected -ne $actual) {
    Remove-Item $installed
    if (Test-Path $retired) { Move-Item $retired $installed }
    throw "the installed copy does not match the build ($actual vs $expected); the previous version was restored"
}

if (Test-Path $retired) {
    Remove-Item $retired -ErrorAction SilentlyContinue
}

& $installed --version
Write-Output "installed $installed (sha256 $actual)"

if ($Register) {
    claude mcp remove deckhand -s user 2>$null | Out-Null
    claude mcp add -s user deckhand -- $installed mcp
    if ($LASTEXITCODE -ne 0) { throw "claude mcp add failed" }
    Write-Output "registered with Claude Code as 'deckhand'; restart Claude Code sessions to load it"
}
