<#
.SYNOPSIS
    Removes MX4 Haptic installed by install.ps1.

.DESCRIPTION
    Stops the app, removes autostart, the Start menu shortcut and the
    installed exe. The configuration in %LOCALAPPDATA%\MX4Haptic is kept
    unless -RemoveConfig is given.
        powershell -ExecutionPolicy Bypass -File .\uninstall.ps1
#>
param(
    # Also delete %LOCALAPPDATA%\MX4Haptic\config.toml.
    [switch]$RemoveConfig
)

$ErrorActionPreference = 'Stop'

$InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\MX4Haptic'
$ConfigDir  = Join-Path $env:LOCALAPPDATA 'MX4Haptic'
$RunKey     = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$RunValue   = 'MX4HapticDaemon'
$Shortcut   = Join-Path ([Environment]::GetFolderPath('Programs')) 'MX4 Haptic.lnk'

$running = Get-Process -Name 'mx4-haptic-daemon' -ErrorAction SilentlyContinue
if ($running) {
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}

Remove-ItemProperty -Path $RunKey -Name $RunValue -ErrorAction SilentlyContinue
Remove-Item -Path $Shortcut -Force -ErrorAction SilentlyContinue
Remove-Item -Path $InstallDir -Recurse -Force -ErrorAction SilentlyContinue

if ($RemoveConfig) {
    Remove-Item -Path $ConfigDir -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host 'Removed MX4 Haptic and its configuration.'
} else {
    Write-Host "Removed MX4 Haptic. Configuration kept in $ConfigDir"
}
