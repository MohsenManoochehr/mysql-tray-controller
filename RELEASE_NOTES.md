@'

# MySQL Tray Controller v1.0.0

The first stable release of MySQL Tray Controller.

## Features

- Live MySQL and MariaDB Windows service monitoring
- Professional color-coded database status icons
- Start, stop, and restart service controls
- Automatic detection of common MySQL and MariaDB service names
- Configurable service name and refresh interval
- Optional startup with Windows
- UAC elevation only when changing the service state
- Built-in version, author, repository, and diagnostics information
- Native headless Windows application
- No MySQL username or password required

## Installation

### Portable use

1. Download the Windows x64 ZIP.
2. Extract it to a permanent folder.
3. Run `mysql-tray-controller.exe`.

### Install for the current Windows user

Open PowerShell inside the extracted folder and run:

    powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -SkipBuild

## Supported service names

- MySQL84
- MySQL80
- MySQL
- MariaDB
- wampmysqld64
- wampmariadb64

## Notes

The executable is currently unsigned. Windows SmartScreen may display a warning on first launch.

Created by Mohsen Manoochehr.
'@ | Set-Content ".\RELEASE_NOTES.md" -Encoding UTF8
