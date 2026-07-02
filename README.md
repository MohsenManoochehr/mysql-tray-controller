# MySQL Tray Controller

A small, headless Windows tray application written in Rust.

## Behavior

- Green tray icon: the configured MySQL Windows service is running.
- Red tray icon: the service is stopped.
- Yellow tray icon: the service is starting, stopping, or changing state.
- Blue tray icon: the service is paused.
- Gray tray icon: the service was not found or its status could not be read.
- Left-click or right-click the tray icon to open the menu.
- Choose **Running**, **Stopped**, or **Restart MySQL**.
- Windows shows a UAC prompt only when changing the service state.
- The tray app itself runs without administrator privileges.
- Optional startup with Windows is available from the tray menu.
- The app refreshes the service state automatically.

## Default service detection

On first launch, the app checks these service names:

1. `MySQL84`
2. `MySQL80`
3. `MySQL`
4. `MariaDB`
5. `mariadb`
6. `wampmysqld64`
7. `wampmariadb64`

If none is found, it defaults to `MySQL84`.

## Configuration

The configuration file is created here:

```text
%APPDATA%\MySQLTrayController\config.ini
```

Example:

```ini
service_name=MySQL84
refresh_interval_seconds=2
```

Use the Windows service **name**, not only its display label. You can find it in:

```text
services.msc -> your MySQL service -> Properties -> Service name
```

After editing the file, choose **Reload configuration** from the tray menu.

Errors are logged to:

```text
%APPDATA%\MySQLTrayController\error.log
```

## Prerequisites

Install Rust with the MSVC toolchain:

```powershell
winget install --id Rustlang.Rustup --exact --source winget
rustup default stable-x86_64-pc-windows-msvc
```

Rust's MSVC toolchain also needs Microsoft C++ Build Tools. In the Visual Studio Installer, select:

```text
Desktop development with C++
MSVC build tools
Windows SDK
```

## Run during development

From the project directory:

```powershell
cargo run
```

Debug builds keep a console window available for diagnostics.

## Build the headless release

```powershell
cargo build --release
```

The executable will be:

```text
target\release\mysql-tray-controller.exe
```

Release builds do not show a console window.

## Install locally

PowerShell may block local scripts. Run the installer with:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

The script:

- builds the release executable,
- copies it to `%LOCALAPPDATA%\Programs\MySQLTrayController`,
- creates a Start Menu shortcut,
- launches the tray app.

To install an already-built executable:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -SkipBuild
```

## Uninstall

First exit the app from its tray menu, then run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall.ps1
```

The configuration directory is intentionally preserved.
