# Dune Dedicated Server Manager

A Windows-first manager for the Dune Awakening Playtest dedicated server.

![Dune Dedicated Server Manager](images/screenshot.png)

The app provisions and manages Dune dedicated servers through a Rust core library, managed local tools, Hyper-V or remote Ubuntu setup, SSH bootstrap, and Kubernetes control commands.

## Installation Guide

1. Download the latest Windows installer from GitHub Releases.
2. Run the installer.
3. Start Dune Dedicated Server Manager from the Start menu or installed shortcut.
4. Approve the Windows administrator prompt when using local Hyper-V setup.
5. Create a new server from the app.

The app guides you through detecting the host, choosing a server layout, entering your Self-Host Service Token, and starting provisioning.

## Server Setup Guides

- [Remote Ubuntu setup guide](docs/ubuntu-setup-guide.md)
- [Local Hyper-V setup guide](docs/hyper-v-setup-guide.md)

Required game ports:

- UDP 7777-7810 for game servers
- TCP 31982 for RMQ

## Building From Source

See [Building From Source](docs/building-from-source.md).

## License

MIT License. See [LICENSE](LICENSE).
