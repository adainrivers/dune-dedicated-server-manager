# Building From Source

## Prerequisites

- Rust stable
- Node.js 22
- npm
- Windows with WebView2 runtime
- Podman or Docker for Linux AppImage builds
- Git

## Install dependencies

```powershell
cd app
npm ci
```

## Run checks

```powershell
cargo check --workspace
cargo test --workspace
cargo doc -p dune-manager-core --no-deps
cd app
npm run build
```

## Run the desktop app in development

```powershell
cd app
npm run tauri -- dev
```

## Build the Windows installer

```powershell
cd app
npm run tauri -- build
```

The unsigned local build creates the application executable and NSIS installer under `target/release`.

## Build the Linux AppImage

The repository includes a `Dockerfile` for an Ubuntu 26.04 build container with Node.js 22, stable Rust, and Tauri Linux dependencies.

```bash
podman build -t dune-manager-appimage .
podman run --rm --userns=keep-id -v "$PWD:/workspace:Z" dune-manager-appimage
```

The AppImage is written to `target/release/bundle/appimage/`. Use Docker with the same `build` and `run` arguments if Podman is not available.
