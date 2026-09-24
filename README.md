# Dune Dedicated Server Manager

A desktop manager for existing Dune Awakening dedicated servers.

![Dashboard — BattleGroup status, lifecycle actions, management service, and tunnel controls](images/ss-1.png)

The app manages already-provisioned Dune dedicated servers over SSH and
Kubernetes control commands. It does not install the game server, create VMs,
configure Hyper-V, provision Ubuntu, or manage external tools such as SteamCMD.

## Features

- Remote server profile management with SSH private-key authentication
- **Dashboard**: BattleGroup status, start, stop, and restart controls, read from live Kubernetes state, with gateway health, per-server uptime, disk usage for the root and k3s storage filesystems, and optional 30-second auto-refresh
- **Update**: installed vs. available game build and a one-click server update that downloads any newer Steam build
- **Pods**: per-component health, live log tails, and safe per-pod restarts
- Secure Director, File Browser, PostgreSQL, and PgHero access through local SSH tunnels, plus your own custom tunnels to any port on the host
- Bundled `dune-server-service` daemon for on-host scheduled maintenance (daily restarts with in-game warnings, automated backups, server update check + apply), installed over SSH straight from the Management card on systemd (Ubuntu) and OpenRC (Alpine) hosts
- **Users**: player list with online filter and auto-refresh (remembered, and paused while the BattleGroup is stopped), and a shortcut into the Admin tab for a selected player
- **Admin**: console for in-game actions: item grants, service broadcasts, kicks, teleports, vehicle spawns, XP and skill changes, water refills, inventory and progression resets, player lookup with live pawn location, and a logged history of every published command
- **Automated tasks**: separate enable switches for auto restart, auto update, and auto backup, which never overlap (each waits for the one in progress); editable schedules (daily restart time, warning lead/frequency, update apply lead, backup cron, IANA timezone); recent run history; and cleanup of finished database operations. Saving restarts the service so changes apply immediately
- **Welcome Package**: automatically gives new players a set of backpack items (water containers arrive full) and an optional welcome whisper. Items are written directly to the game database, tracked in the management service's SQLite ledger, and failed deliveries can be retried. Configure it with a visual editor or raw JSON
- In-app update check that shows the release notes before you install

![Admin tab — granting items to online players with a searchable Funcom item picker](images/ss-2.png)

More management features coming soon.

## Install

Download the latest release for your operating system from GitHub Releases.

- Windows: run the NSIS installer.
- Linux: use the AppImage or Debian package.
- macOS: use the DMG for your Mac architecture.

After launching the app, add an existing server profile with its host, SSH user,
and private key path, then refresh it to detect BattleGroups and management
endpoints.

The Users, Admin, Welcome Package, and Automated tasks tabs need the on-host
management service. To install it, open the server's Management Service card
and click **Install**.

### After updating the app

Most releases also update the on-host `dune-server-service`. After installing a
new app version, open each server's Management Service card and click
**Update** (it appears when the host service is older than the app). Each
release's notes say whether this step is required.

## Managed Server Assumptions

The target server must already be installed and reachable over SSH. The app
expects the Dune Kubernetes resources and vendor management scripts to exist on
the server before you add it.

To set up a fresh Ubuntu host by hand, see the
[Manual Ubuntu Server Setup Guide](docs/ubuntu-manual-setup-guide.md).

Required player-facing/server ports depend on your own server deployment. A
typical dedicated-server deployment uses:

- UDP 7777-7810 for game servers
- TCP 31982 for RMQ

If you found a bug or are having other issues, please create an issue here:
https://github.com/adainrivers/dune-dedicated-server-manager/issues

## Building From Source

See [Building From Source](docs/building-from-source.md).

## License

MIT License. See [LICENSE](LICENSE).
