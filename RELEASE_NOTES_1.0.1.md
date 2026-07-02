# MySQL Tray Controller v1.0.1

A maintenance release containing service-control and installation improvements.

## Fixed

- Wait for a stopping MySQL service to fully stop before restarting it.
- Support installation from extracted release packages.
- Support installation from locally built source using `-SkipBuild`.

## Installation

Download the Windows x64 ZIP, extract it, and run:

`mysql-tray-controller.exe`

To install it for the current Windows user:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -SkipBuild
```
