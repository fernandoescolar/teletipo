[CmdletBinding()]
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\Teletipo"
)

$ErrorActionPreference = "Stop"

if (Test-Path $InstallDir) {
    Remove-Item -Path $InstallDir -Recurse -Force
}

$programs = [Environment]::GetFolderPath("Programs")
$shortcutPath = Join-Path $programs "Teletipo.lnk"
if (Test-Path $shortcutPath) {
    Remove-Item $shortcutPath -Force
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath) {
    $parts = $userPath -split ';' | Where-Object { $_ -and $_.Trim() -ne "" -and $_ -ne $InstallDir }
    [Environment]::SetEnvironmentVariable("Path", ($parts -join ';'), "User")
}

Write-Host "[teletipo] uninstall complete"
