# Building From Source

## Repository Layout

- `crates/dune-manager-core`: Rust core library and orchestration used by the desktop app.
- `crates/dune-server-service`: on-host Linux management daemon (scheduler + JSON API). The desktop app bundles it and installs it on servers over SSH.
- `app/src-tauri`: Tauri 2 desktop shell.
- `app/src`: React + TypeScript UI.

## Prerequisites

- Rust stable
- Node.js 22
- npm
- Git

To build the bundled `dune-server-service` Linux binary you also need:

- The `x86_64-unknown-linux-musl` Rust target
- [Zig](https://ziglang.org/) (CI uses 0.13.0) and `cargo-zigbuild`

Platform-specific desktop dependencies:

- Windows: WebView2 runtime
- Linux: WebKitGTK 4.1, AppIndicator, librsvg, patchelf, pkg-config, and OpenSSL development headers
- macOS: Xcode Command Line Tools

On Ubuntu 22.04, install the Linux desktop build dependencies with:

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf pkg-config libssl-dev
```

## Install Frontend Dependencies

```bash
cd app
npm ci
```

## Run Checks

These match the CI workflow:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo doc -p dune-manager-core --no-deps
cd app
npm run build
```

## Build The Bundled Management Service

The desktop app ships `dune-server-service` as a Tauri resource, together with
its systemd unit and OpenRC init script. Release CI builds these in the
`linux-service-binary` job. To build them locally (Windows PowerShell shown):

```powershell
rustup target add x86_64-unknown-linux-musl
cargo install --locked cargo-zigbuild
cargo zigbuild -p dune-server-service --release --target x86_64-unknown-linux-musl
Copy-Item target\x86_64-unknown-linux-musl\release\dune-server-service `
  app\src-tauri\binaries\dune-server-service -Force
Copy-Item crates\dune-server-service\systemd\dune-server-service.service `
  app\src-tauri\binaries\dune-server-service.service -Force
Copy-Item crates\dune-server-service\openrc\dune-server-service `
  app\src-tauri\binaries\dune-server-service.openrc -Force
```

The files in `app/src-tauri/binaries` are not tracked by git. Rebuild and copy
the binary whenever the service code or its version changes. Otherwise the app
will install a stale service on your servers.

On Windows, `scripts\rebuild-release.cmd` runs the whole local release chain in
one go: workspace tests, frontend build, musl service build, binary copy, and
the NSIS installer. It copies only the binary, so run the unit/init-script
copies above once on a fresh clone.

## Run In Development

```bash
cd app
npm run tauri -- dev
```

On Linux the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` automatically to avoid
a WebKitGTK 4.1 crash on GNOME Wayland (`Error 71 dispatching to Wayland
display`). Export the variable yourself with a different value to override.

## Build A Local Production App

For a local production executable without updater signing or release bundling:

```bash
cd app
npm run tauri -- build --no-bundle
```

The executable is written under the workspace `target/release` directory.

## Build Installers

Release packaging is normally handled by GitHub Actions when a version tag is
pushed. Local full packaging may require platform-specific signing or installer
setup.

Common local bundle commands:

```bash
cd app
npm run tauri -- build --bundles nsis
npm run tauri -- build --bundles appimage,deb
npm run tauri -- build --bundles dmg
```

The app manages already-provisioned servers only. Building from source does not
add any server setup, provisioning, Hyper-V, or installer workflow to the app.
