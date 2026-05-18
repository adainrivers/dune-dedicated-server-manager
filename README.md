# Dune Dedicated Server Manager

A Windows-first manager for the Dune Awakening Playtest dedicated server, with an experimental Linux AppImage build for remote-server workflows.

![Dune Dedicated Server Manager](images/screenshot.png)

The app provisions and manages Dune dedicated servers through a Rust core library, managed local tools, Hyper-V or remote Ubuntu setup, SSH bootstrap, and Kubernetes control commands.

## Features

- Local Hyper-V based server provisioning
- Remote Ubuntu server provisioning via SSH
- Basic server management capabilities such as log viewer, secure Director and File Browser access via SSH

More management features coming soon.

## Installation Guide

1. Download the latest Windows installer from GitHub Releases.
2. Run the installer.
3. Start Dune Dedicated Server Manager from the Start menu or installed shortcut.
4. Approve the Windows administrator prompt when using local Hyper-V setup.
5. Create a new server from the app.

Linux AppImage builds are currently intended for testing and remote Ubuntu or Proxmox management. Local Hyper-V setup remains Windows-only.

The app guides you through detecting the host, choosing a server layout, entering your Self-Host Service Token, and starting provisioning.

If you found a bug or are having other issues, please create an issue here:
https://github.com/adainrivers/dune-dedicated-server-manager/issues

## Server Setup Guides

- [Remote Ubuntu setup guide](docs/ubuntu-setup-guide.md)
- [Manual Ubuntu setup guide](docs/ubuntu-manual-setup-guide.md)
- [Local Hyper-V setup guide](docs/hyper-v-setup-guide.md)

Required game ports:

- UDP 7777-7810 for game servers
- TCP 31982 for RMQ

## Building From Source

See [Building From Source](docs/building-from-source.md).

## License

MIT License. See [LICENSE](LICENSE).
