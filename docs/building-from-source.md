# Building From Source

## Prerequisites

- Rust stable
- Node.js 22
- npm
- Windows with WebView2 runtime
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
