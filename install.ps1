<#
.SYNOPSIS
    Builds and installs MX4 Haptic for the current Windows user.

.DESCRIPTION
    - stops a running instance
    - builds the release binary with cargo (skip with -Binary <path>)
    - copies the exe to %LOCALAPPDATA%\Programs\MX4Haptic
    - adds a Start menu shortcut
    - enables autostart at login (starts minimized to the tray; skip with -NoAutostart)
    - starts the app in the tray

    No administrator rights are needed. Run from the repository root:
        powershell -ExecutionPolicy Bypass -File .\install.ps1
#>
param(
    # Install an already built exe instead of running cargo build.
    [string]$Binary,
    # Do not register the app to start at login.
    [switch]$NoAutostart
)

$ErrorActionPreference = 'Stop'

$ExeName     = 'mx4-haptic-daemon.exe'
$InstallDir  = Join-Path $env:LOCALAPPDATA 'Programs\MX4Haptic'
$InstalledExe = Join-Path $InstallDir $ExeName
$RunKey      = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$RunValue    = 'MX4HapticDaemon'
$Shortcut    = Join-Path ([Environment]::GetFolderPath('Programs')) 'MX4 Haptic.lnk'

# Stop first: a running copy (e.g. started from target\release) locks the exe.
Write-Host '==> Stopping running instances...'
$running = Get-Process -Name 'mx4-haptic-daemon' -ErrorAction SilentlyContinue
if ($running) {
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}

if (-not $Binary) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'cargo was not found. Install Rust from https://rustup.rs and run this script again.'
    }
    Write-Host '==> Building release binary (cargo build --release)...'
    Push-Location $PSScriptRoot
    # cargo prints progress to stderr; with 'Stop' Windows PowerShell 5.1
    # would turn those lines into terminating errors when output is redirected.
    $ErrorActionPreference = 'Continue'
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        $ErrorActionPreference = 'Stop'
        Pop-Location
    }
    $Binary = Join-Path $PSScriptRoot "target\release\$ExeName"
}
if (-not (Test-Path $Binary)) { throw "Binary not found: $Binary" }

Write-Host "==> Installing to $InstallDir"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Path $Binary -Destination $InstalledExe -Force

Write-Host '==> Creating Start menu shortcut'
$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut($Shortcut)
$lnk.TargetPath = $InstalledExe
$lnk.WorkingDirectory = $InstallDir
$lnk.Description = 'MX Master 4 haptic feedback'
$lnk.Save()

if ($NoAutostart) {
    Remove-ItemProperty -Path $RunKey -Name $RunValue -ErrorAction SilentlyContinue
} else {
    Write-Host '==> Enabling autostart (minimized to tray)'
    Set-ItemProperty -Path $RunKey -Name $RunValue -Value "`"$InstalledExe`" --minimized"
}

Write-Host '==> Starting MX4 Haptic in the tray'
Start-Process -FilePath $InstalledExe -ArgumentList '--minimized' -WorkingDirectory $InstallDir

Write-Host ''
Write-Host "Installed $InstalledExe"
Write-Host 'Click the tray icon (blue circle, may be under the ^ arrow) to open settings.'
