<div align="center">

# Techomato Orbit

<img src="assets/brand/orbit-icon.svg" width="128" alt="Techomato Orbit icon">

**Your local development machine, organized in one window.**

[![Windows](https://img.shields.io/badge/Platform-Windows-8B5CF6?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/KoushikMallik-developer/techomato-orbit/releases)
[![Latest Release](https://img.shields.io/badge/Latest%20Release-v0.2.0-06B6D4?style=for-the-badge)](https://github.com/KoushikMallik-developer/techomato-orbit/releases/latest)
[![License](https://img.shields.io/badge/License-Free-F59E0B?style=for-the-badge)](#license)

</div>

Techomato Orbit is a free Windows desktop app for managing local services, one-click databases,
projects, logs, ports, and PC health. It runs in the system tray and works offline after setup,
except when downloading a database or checking for updates.

> **Windows 10 or 11 - 64-bit**

---

## Download and install

### Latest stable release - recommended

Use the stable build for the most reliable experience.

1. Open the **[Latest Stable Release](https://github.com/KoushikMallik-developer/techomato-orbit/releases/latest)** page.
2. Under **Assets**, download `Techomato Orbit_0.2.0_x64-setup.exe`.
3. Double-click the downloaded installer.
4. Choose **Install for me only** or **Install for all users**.
5. Launch **Techomato Orbit** from the Start menu.

> **Windows SmartScreen:** Orbit currently has no paid Windows publisher certificate. If Windows
> displays **Microsoft Defender SmartScreen protected your PC**, select **More info**, then
> **Run anyway**.

### Latest release version

Use this page to see every published version. The newest entry may be a prerelease.

1. Open the **[All Releases](https://github.com/KoushikMallik-developer/techomato-orbit/releases)** page.
2. Choose the newest release you want to use. A **Pre-release** label means it may be less stable.
3. Open **Assets** and download `Techomato Orbit_0.2.0_x64-setup.exe` for release `v0.2.0`,
   or replace the version with the version you chose.
4. Run the installer and follow the on-screen instructions.

**MSI installer:** Organizations and managed PCs can use
`Techomato Orbit_0.2.0_x64_en-US.msi` for version `0.2.0`. It is machine-wide and may require
administrator rights.

### Upgrade an existing installation

Open Orbit, go to **Settings > Updates**, and use **Check for updates**. Updates are signature
verified and are installed only after you confirm.

---

## What you can do

|               |                                                                                      |
| :------------ | :----------------------------------------------------------------------------------- |
| **Services**  | Start, stop, restart, monitor health, and view live logs for local services.         |
| **Databases** | Add PostgreSQL, MariaDB, Redis, or MongoDB with one click - no Docker required.      |
| **Projects**  | Group services and start or stop dependencies in the correct order.                  |
| **Ports**     | See the actual port used by every running service.                                   |
| **Doctor**    | Check drives, memory, caches, startup apps, power settings, WSL, Docker, and Ollama. |
| **Tray mode** | Close the window while Orbit stays available from the system tray.                   |

### Handy shortcuts

- Press **Ctrl+K** to open the command palette.
- Add global shortcuts from Settings.
- Configure notifications and start Orbit with Windows.

---

## Databases and local data

Orbit downloads databases once, verifies the download, and keeps the program and data under:

```text
%LOCALAPPDATA%\Techomato Orbit
```

Databases listen on this PC only and do not use passwords, so they are intended for development.
Deleting a database service removes its data. Uninstalling Orbit does not remove database files;
delete the folder above if you want to remove every database completely.

Orbit also discovers compatible databases already running on Windows, including Redis,
PostgreSQL, MySQL/MariaDB, and MongoDB.

---

## Build from source

### Prerequisites

- Node.js 22.12 or newer
- Rust with rustup
- Visual Studio C++ Build Tools
- Windows 10 or 11, 64-bit

### Development

```bash
npm ci
npm start
```

### Tests

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

### Build the Windows installers

```bash
npm run release
```

Installers are created under `src-tauri/target/release/bundle/`. The setup and release process is
documented in [docs/releasing.md](docs/releasing.md).

---

## Project links

- [Architecture](docs/architecture.md)
- [Database schema](docs/database_schema.md)
- [Design specification](docs/superpowers/specs/2026-11-19-techomato-orbit-design.md)
- [Release guide](docs/releasing.md)

## License

Techomato Orbit is free to use.
