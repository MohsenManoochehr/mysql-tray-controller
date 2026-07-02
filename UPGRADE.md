# Upgrade to the polished icon build

1. Exit the currently running tray app from its tray menu.
2. Open PowerShell in the project directory.
3. Build the updated executable:

```powershell
cargo fmt
cargo build --release
```

4. Test the freshly built executable directly:

```powershell
.\target\release\mysql-tray-controller.exe
```

5. Exit that test instance, then update the installed copy and Start Menu shortcut:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -SkipBuild
```

The installer stops an older running copy, replaces the installed executable,
recreates the Start Menu shortcut, and explicitly selects the embedded icon.

If File Explorer still displays the old executable icon, rename the executable
temporarily or restart Windows Explorer. The live tray icon itself should update
immediately when the new executable runs.
