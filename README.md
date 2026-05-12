# Dune Dedicated Server Manager

A Windows-first manager for the Dune Awakening Playtest dedicated server.

The app provisions and manages the vendor Hyper-V server package through a Rust core library, local managed tools, guest SSH bootstrap, and a VM-side Manager API.

## Installation

1. Download the latest Windows installer from GitHub Releases.
2. Run the installer.
3. Start Dune Dedicated Server Manager from the Start menu or installed shortcut.
4. Approve the Windows administrator prompt.
5. In the app, configure:
   - Self-Host Service Token
   - World name and region
   - Hagga Basin, Social Hubs, and Deep Desert layout
   - VM memory, disk size, location, and network settings
6. Click Start Full Setup.

The setup flow installs app-owned copies of SteamCMD and OpenSSH, downloads the server package, imports the Hyper-V VM, bootstraps the guest, configures k3s resources, applies the selected world layout, and starts the battlegroup.

## Requirements

- Windows 10 or Windows 11 with Hyper-V support.
- Administrator access.
- Virtualization enabled in BIOS or UEFI.
- Hyper-V installed and running.
- Enough physical memory for the selected layout.
- Enough disk space for the VM destination.
- A Dune Awakening Self-Host Service Token.

If you choose an external player-facing IP, forward these ports to the VM:

- 7777-7810 UDP for game servers.
- 31982 TCP for RMQ.

## Auto Update

Release builds check GitHub Releases for signed updates. The app asks before installing an update, then relaunches after installation.

## Building From Code

Prerequisites:

- Rust stable
- Node.js 22
- npm
- Windows with the WebView2 runtime
- Git

Install dependencies:

```powershell
cd app
npm ci
```

Run local checks:

```powershell
cargo check --workspace
cargo test --workspace
cargo doc -p dune-manager-core --no-deps
cd app
npm run build
```

Run the desktop app in development:

```powershell
cd app
npm run tauri -- dev
```

Build the Windows installer:

```powershell
cd app
npm run tauri -- build
```

Build the VM-side Manager API for Linux from WSL:

```powershell
wsl -d Ubuntu-22.04 -- bash -lc "cd /mnt/f/Dune/Development/DedicatedServerManager/manager-api && cargo build --target x86_64-unknown-linux-musl --release"
```
