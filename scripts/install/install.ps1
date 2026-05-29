[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\Teletipo",
    [switch]$AddToPath = $true,
    [switch]$Shortcut = $true,
    [switch]$NoVerify,
    [string]$FromArchive,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"

$Repo = "fernandoescolar/teletipo"
$BaseReleaseUrl = "https://github.com/$Repo/releases"

function Write-Log {
    param([string]$Message)
    Write-Host "[teletipo] $Message"
}

function Resolve-LatestTag {
    $response = Invoke-WebRequest -Uri "$BaseReleaseUrl/latest" -MaximumRedirection 0 -ErrorAction SilentlyContinue
    if ($response.StatusCode -ge 300 -and $response.StatusCode -lt 400) {
        return [System.IO.Path]::GetFileName($response.Headers.Location)
    }
    $final = $response.BaseResponse.ResponseUri.AbsoluteUri
    return [System.IO.Path]::GetFileName($final)
}

function Ensure-UserPath {
    param([string]$PathToAdd)

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if ($userPath) {
        $parts = $userPath -split ';' | Where-Object { $_ -and $_.Trim() -ne "" }
    }

    if ($parts -notcontains $PathToAdd) {
        $newPath = ($parts + $PathToAdd) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Log "added to user PATH: $PathToAdd"
    }
}

function New-StartMenuShortcut {
    param(
        [string]$ExePath,
        [string]$IconPath
    )

    $programs = [Environment]::GetFolderPath("Programs")
    if (-not (Test-Path $programs)) {
        New-Item -ItemType Directory -Path $programs -Force | Out-Null
    }

    $shortcutPath = Join-Path $programs "Teletipo.lnk"
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = $ExePath
    $shortcut.WorkingDirectory = Split-Path $ExePath -Parent
    if (Test-Path $IconPath) {
        $shortcut.IconLocation = $IconPath
    }
    $shortcut.Save()

    Write-Log "shortcut created: $shortcutPath"
}

function Invoke-Uninstall {
    param([string]$Dir)

    if (Test-Path $Dir) {
        Remove-Item -Path $Dir -Recurse -Force
    }

    $programs = [Environment]::GetFolderPath("Programs")
    $shortcutPath = Join-Path $programs "Teletipo.lnk"
    if (Test-Path $shortcutPath) {
        Remove-Item $shortcutPath -Force
    }

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath) {
        $parts = $userPath -split ';' | Where-Object { $_ -and $_.Trim() -ne "" -and $_ -ne $Dir }
        [Environment]::SetEnvironmentVariable("Path", ($parts -join ';'), "User")
    }

    Write-Log "uninstall complete"
}

if ($Uninstall) {
    Invoke-Uninstall -Dir $InstallDir
    exit 0
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("teletipo-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

try {
    $work = Join-Path $tmp "work"
    New-Item -ItemType Directory -Path $work -Force | Out-Null

    if ([string]::IsNullOrWhiteSpace($FromArchive)) {
        if ($Version -eq "latest") {
            $Version = Resolve-LatestTag
        }

        $asset = "teletipo-windows-x86_64.zip"
        $zipPath = Join-Path $tmp $asset
        $sumPath = Join-Path $tmp "SHA256SUMS"

        Invoke-WebRequest -Uri "$BaseReleaseUrl/download/$Version/$asset" -OutFile $zipPath

        if (-not $NoVerify) {
            Invoke-WebRequest -Uri "$BaseReleaseUrl/download/$Version/SHA256SUMS" -OutFile $sumPath
            $sumLine = Select-String -Path $sumPath -Pattern "\s$asset$" | Select-Object -First 1
            if (-not $sumLine) {
                throw "missing checksum for $asset"
            }
            $expected = ($sumLine.Line -split '\s+')[0]
            $actual = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actual -ne $expected.ToLowerInvariant()) {
                throw "checksum mismatch for $asset"
            }
        }

        Expand-Archive -Path $zipPath -DestinationPath $work -Force
    } else {
        Copy-Item -Path (Join-Path $FromArchive "*") -Destination $work -Recurse -Force
    }

    $exeCandidate = Get-ChildItem -Path $work -Filter "teletipo.exe" -Recurse | Select-Object -First 1
    if (-not $exeCandidate) {
        throw "teletipo.exe not found"
    }

    $iconCandidate = Get-ChildItem -Path $work -Filter "teletipo.png" -Recurse | Select-Object -First 1

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item $exeCandidate.FullName (Join-Path $InstallDir "teletipo.exe") -Force
    if ($iconCandidate) {
        Copy-Item $iconCandidate.FullName (Join-Path $InstallDir "teletipo.png") -Force
    }

    if ($AddToPath) {
        Ensure-UserPath -PathToAdd $InstallDir
    }

    if ($Shortcut) {
        New-StartMenuShortcut -ExePath (Join-Path $InstallDir "teletipo.exe") -IconPath (Join-Path $InstallDir "teletipo.png")
    }

    Write-Log "installed in $InstallDir"
}
finally {
    if (Test-Path $tmp) {
        Remove-Item -Path $tmp -Recurse -Force
    }
}
