param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$ReleaseExe = Join-Path $ProjectRoot "target\release\mysql-tray-controller.exe"
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\MySQLTrayController"
$InstalledExe = Join-Path $InstallDir "mysql-tray-controller.exe"
$StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$ShortcutPath = Join-Path $StartMenuDir "MySQL Tray Controller.lnk"

if (-not $SkipBuild) {
    Push-Location $ProjectRoot
    try {
        cargo build --release
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $ReleaseExe)) {
    throw "Release executable not found: $ReleaseExe"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $ReleaseExe $InstalledExe -Force

$Shell = New-Object -ComObject WScript.Shell
$Shortcut = $Shell.CreateShortcut($ShortcutPath)
$Shortcut.TargetPath = $InstalledExe
$Shortcut.WorkingDirectory = $InstallDir
$Shortcut.Description = "Control and monitor the local MySQL Windows service"
$Shortcut.Save()

Start-Process $InstalledExe

Write-Host ""
Write-Host "Installed successfully:"
Write-Host "  $InstalledExe"
Write-Host ""
Write-Host "Use the tray menu to enable 'Start tray app with Windows'."
