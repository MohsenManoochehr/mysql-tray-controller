param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$BuiltExe = Join-Path $ProjectRoot "target\release\mysql-tray-controller.exe"
$PackagedExe = Join-Path $ProjectRoot "mysql-tray-controller.exe"
$ReleaseExe = $BuiltExe
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\MySQLTrayController"
$InstalledExe = Join-Path $InstallDir "mysql-tray-controller.exe"
$StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$ShortcutPath = Join-Path $StartMenuDir "MySQL Tray Controller.lnk"

if ($SkipBuild) {
    if (Test-Path $PackagedExe) {
        # Extracted GitHub release package
        $ReleaseExe = $PackagedExe
    }
    elseif (Test-Path $BuiltExe) {
        # Locally built source repository
        $ReleaseExe = $BuiltExe
    }
    else {
        throw @"
No executable was found.

Expected one of:
  $PackagedExe
  $BuiltExe

Run 'cargo build --release' first, or use the extracted GitHub release package.
"@
    }
}
else {
    Push-Location $ProjectRoot
    try {
        cargo build --release
        $ReleaseExe = $BuiltExe
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $ReleaseExe)) {
    throw "Release executable not found: $ReleaseExe"
}

Get-Process "mysql-tray-controller" -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $ReleaseExe $InstalledExe -Force

Remove-Item $ShortcutPath -Force -ErrorAction SilentlyContinue

$Shell = New-Object -ComObject WScript.Shell
$Shortcut = $Shell.CreateShortcut($ShortcutPath)
$Shortcut.TargetPath = $InstalledExe
$Shortcut.WorkingDirectory = $InstallDir
$Shortcut.IconLocation = "$InstalledExe,0"
$Shortcut.Description = "Control and monitor the local MySQL Windows service"
$Shortcut.Save()

Start-Process $InstalledExe

Write-Host ""
Write-Host "Installed successfully:"
Write-Host "  $InstalledExe"
Write-Host ""
Write-Host "Use the tray menu to enable 'Start tray app with Windows'."
