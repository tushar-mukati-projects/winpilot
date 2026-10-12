param(
    [switch]$SkipBuild,
    [switch]$Register,
    [switch]$Uninstall,
    [switch]$Purge,
    [ValidateSet("tiny", "base", "small")]
    [string]$SpeechModel
)

$ErrorActionPreference = "Stop"
$repo = Join-Path $PSScriptRoot ".."
$built = Join-Path $repo "target\release\deckhand.exe"
$icon = Join-Path $repo "assets\deckhand.png"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\deckhand"
$installed = Join-Path $installDir "deckhand.exe"
$installedIcon = Join-Path $installDir "deckhand.png"
$appId = "Deckhand.Agent"
$appIdKey = "HKCU:\Software\Classes\AppUserModelId\$appId"
$configDir = Join-Path $env:APPDATA "deckhand"
$logDir = Join-Path $env:LOCALAPPDATA "deckhand"
$modelDir = Join-Path $logDir "models"
$speechModelSource = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main"
$speechModelHashes = @{
    tiny  = "BE07E048E1E599AD46341C8D2A135645097A538221678B7ACDD1B1919C6E1B21"
    base  = "60ED5BC3DD14EEA856493D334349B405782DDCAF0028D4B5DF4088345FBA2EFE"
    small = "1BE3A9B2063867B937E64E2EC7483364A79917E157FA98C5D94B5C1FFFEA987B"
}

function Remove-RetiredCopies {
    Get-ChildItem -Path $installDir -Filter "deckhand.old-*.exe" -ErrorAction SilentlyContinue | ForEach-Object {
        Remove-Item $_.FullName -ErrorAction SilentlyContinue
    }
}

function Restore-Retired($retired) {
    if (Test-Path $installed) { Remove-Item $installed -Force -ErrorAction SilentlyContinue }
    if (-not $retired) { return }
    try {
        Move-Item $retired $installed -Force
    }
    catch {
        throw "the previous deckhand.exe could not be put back; it is at $retired ($_)"
    }
    Write-Warning "the new build was not installed; the previous deckhand.exe was put back"
}

function Copy-Verified {
    Copy-Item $built $installed
    $expected = (Get-FileHash $built -Algorithm SHA256).Hash
    $actual = (Get-FileHash $installed -Algorithm SHA256).Hash
    if ($expected -ne $actual) { throw "the installed copy does not match the build ($actual vs $expected)" }
    return $actual
}

function Install-Binary {
    $retired = $null
    if (Test-Path $installed) {
        $retired = Join-Path $installDir ("deckhand.old-{0}.exe" -f (Get-Date -Format "yyyyMMdd-HHmmss-fff"))
        Move-Item $installed $retired
    }
    $verified = $false
    try {
        $hash = Copy-Verified
        $verified = $true
        return $hash
    }
    finally {
        if (-not $verified) { Restore-Retired $retired }
    }
}

function Invoke-Native([string]$Command, [string[]]$Arguments) {
    $null = Get-Command $Command
    $ErrorActionPreference = "Continue"
    & $Command @Arguments 2>&1 | ForEach-Object { "$_" }
}

function Test-ClaudeCli {
    return [bool](Get-Command claude -ErrorAction SilentlyContinue)
}

function Remove-ClaudeEntry {
    Invoke-Native "claude" @("mcp", "remove", "deckhand", "-s", "user") | Out-Null
}

function Register-AppIdentity {
    Copy-Item $icon $installedIcon -Force
    New-Item -Path $appIdKey -Force | Out-Null
    New-ItemProperty -Path $appIdKey -Name "DisplayName" -Value "Deckhand" -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $appIdKey -Name "IconUri" -Value $installedIcon -PropertyType String -Force | Out-Null
}

function Remove-McpRegistration {
    if (-not (Test-ClaudeCli)) {
        Write-Output "the claude CLI was not found, so the MCP registration was left alone; remove it with: claude mcp remove deckhand -s user"
        return
    }
    Remove-ClaudeEntry
    Write-Output "removed the 'deckhand' MCP registration (if it existed)"
}

function Test-ModelHash([string]$Path, [string]$Expected) {
    if (-not (Test-Path $Path)) { return $false }
    return (Get-FileHash $Path -Algorithm SHA256).Hash -eq $Expected
}

function Get-SpeechModel([string]$Name, [string]$Partial) {
    $previousProgress = $ProgressPreference
    $ProgressPreference = "SilentlyContinue"
    try {
        Invoke-WebRequest -Uri "$speechModelSource/ggml-$Name.bin" -OutFile $Partial -UseBasicParsing
    }
    finally {
        $ProgressPreference = $previousProgress
    }
}

function Install-SpeechModel([string]$Name) {
    $expected = $speechModelHashes[$Name]
    $target = Join-Path $modelDir "ggml-$Name.bin"
    if (Test-ModelHash $target $expected) {
        Write-Output "the $Name speech model is already installed and verified at $target"
        return
    }
    New-Item -ItemType Directory -Force -Path $modelDir | Out-Null
    $partial = "$target.download"
    if (Test-Path $partial) { Remove-Item $partial -Force }
    Write-Output "downloading the $Name speech model from $speechModelSource"
    Get-SpeechModel $Name $partial
    $actual = (Get-FileHash $partial -Algorithm SHA256).Hash
    if ($actual -ne $expected) {
        Remove-Item $partial -Force
        throw "the downloaded $Name model does not match its pinned SHA-256 ($actual vs $expected); it was deleted, run the command again"
    }
    Move-Item $partial $target -Force
    if (-not (Test-ModelHash $target $expected)) { throw "the installed model at $target failed its hash check after the move; run the command again" }
    Write-Output "installed the $Name speech model at $target (sha256 $expected)"
}

function Uninstall-Deckhand {
    Remove-McpRegistration
    if (Test-Path $appIdKey) { Remove-Item $appIdKey -Recurse -Force }
    Write-Output "removed the $appId notification identity"
    if (Test-Path $installDir) {
        Remove-Item $installDir -Recurse -Force -ErrorAction SilentlyContinue
        if (Test-Path $installDir) {
            Write-Output "some files in $installDir are in use; close Claude Code sessions using Deckhand and run -Uninstall again"
        }
        else {
            Write-Output "removed $installDir"
        }
    }
    if ($Purge) {
        foreach ($dir in @($configDir, $logDir)) {
            if (Test-Path $dir) {
                Remove-Item $dir -Recurse -Force
                Write-Output "removed $dir"
            }
        }
    }
    else {
        Write-Output "kept the config in $configDir and the logs and speech models in $logDir (pass -Purge to remove them)"
    }
}

if ($Uninstall) {
    Uninstall-Deckhand
    return
}
if ($Purge) { throw "-Purge only works together with -Uninstall" }
if ($SpeechModel) {
    Install-SpeechModel $SpeechModel
    return
}

if (-not $SkipBuild) {
    Push-Location $repo
    try {
        Invoke-Native "cargo" @("build", "--release", "-p", "deckhand")
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $built)) { throw "no release build at $built" }
if (-not (Test-Path $icon)) { throw "no icon at $icon" }
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

Remove-RetiredCopies
$hash = Install-Binary
Remove-RetiredCopies
Register-AppIdentity

Invoke-Native $installed @("--version")
Write-Output "installed $installed (sha256 $hash)"
Write-Output "registered the $appId notification identity with the icon $installedIcon"

if ($Register) {
    if (-not (Test-ClaudeCli)) {
        throw "the claude CLI was not found, so Deckhand is installed but not registered; install Claude Code, then run: claude mcp add -s user deckhand -- `"$installed`" mcp"
    }
    Remove-ClaudeEntry
    Invoke-Native "claude" @("mcp", "add", "-s", "user", "deckhand", "--", $installed, "mcp")
    if ($LASTEXITCODE -ne 0) { throw "claude mcp add failed" }
    Write-Output "registered with Claude Code as 'deckhand'; restart Claude Code sessions to load it"
}
