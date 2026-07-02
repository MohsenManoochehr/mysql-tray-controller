$ErrorActionPreference = "Stop"

$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\MySQLTrayController"
$InstalledExe = Join-Path $InstallDir "mysql-tray-controller.exe"
$ShortcutPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\MySQL Tray Controller.lnk"
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"

Get-Process "mysql-tray-controller" -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue

Remove-ItemProperty -Path $RunKey -Name "MySQLTrayController" -ErrorAction SilentlyContinue
Remove-Item $ShortcutPath -Force -ErrorAction SilentlyContinue
Remove-Item $InstallDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "MySQL Tray Controller was uninstalled."
Write-Host "Configuration was kept in:"
Write-Host "  $env:APPDATA\MySQLTrayController"
