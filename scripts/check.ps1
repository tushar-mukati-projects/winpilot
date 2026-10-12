$ErrorActionPreference = "Stop"

function Invoke-Cargo([string[]]$Arguments) {
    $null = Get-Command cargo
    $ErrorActionPreference = "Continue"
    & cargo @Arguments 2>&1 | ForEach-Object { "$_" }
}

Push-Location (Join-Path $PSScriptRoot "..")
try {
    Invoke-Cargo @("fmt", "--all", "--", "--check")
    if ($LASTEXITCODE -eq 0) { Invoke-Cargo @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings") }
    if ($LASTEXITCODE -eq 0) { Invoke-Cargo @("test", "--workspace") }
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
