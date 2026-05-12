# Local Hyper-V Setup Guide

This guide explains how to create a local Windows Hyper-V server with Dune Dedicated Server Manager.

## 1. Prepare Windows

Use Windows 10 or Windows 11 with Hyper-V support.

Before starting setup, make sure:

- Virtualization is enabled in BIOS or UEFI
- Hyper-V is installed
- The Hyper-V vmms service is running
- You have administrator access
- The selected VM location has enough free disk space
- The host has enough physical memory for the selected layout
- A physical IPv4 network adapter with a gateway is active

## 2. Start the app as administrator

Open Dune Dedicated Server Manager. The setup flow requires administrator access for Hyper-V operations such as VM creation, switch configuration, disk resize, and VM lifecycle control.

## 3. Detect local resources

Start a new server setup and select Local Windows Hyper-V.

Click Detect local resources. The app checks Hyper-V readiness, memory, disk space, external IP detection, and supported network adapters.

The rest of the setup form unlocks after detection succeeds.

## 4. Configure the Dune server

Continue through the setup form:

- Enter your Self-Host Service Token
- Choose the world name and region
- Pick the world layout
- Review the memory and CPU requirements
- Choose the VM name and VM location
- Choose the disk size
- Review network settings

The app suggests host network and VM IP settings from the detected adapter.

## 5. Configure port forwarding when needed

If players will connect through your external IP, forward these ports from your router to the VM IP:

- UDP 7777-7810 for game servers
- TCP 31982 for RMQ

If the server is only for LAN testing, local IP mode can be used instead.

## 6. Start creation

Click Start Full Setup.

The app installs managed tools, downloads the server package, creates and configures the Hyper-V VM, bootstraps the guest, configures k3s resources, applies the selected world layout, and starts the battlegroup.

It can take some time for the server to appear in-game.
