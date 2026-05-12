# Remote Ubuntu Setup Guide

This guide explains how to prepare a remote Ubuntu server for Dune Dedicated Server Manager.

## 1. Create an SSH key

Generate an SSH key pair, or use the app's Tools page to generate one.

Keep the private key on your computer. Upload the public key to your server provider during server creation.

## 2. Create a fresh Ubuntu server

Create a fresh Ubuntu 24 or newer VPS or dedicated server. Providers such as Hetzner usually let you add an SSH public key during setup.

Use a server with enough RAM and CPU for the layout you want to run. For wider compatibility, choose IPv4 only and do not enable IPv6.

Do not use a server that already hosts other data or services. Remote setup installs packages, configures k3s, downloads server files, opens service ports, and writes system configuration.

## 3. Configure firewall access

Allow your own IP address to access SSH:

- TCP 22 for SSH

You also need to allow the game ports from any IP:

- UDP 7777-7810 for game servers
- TCP 31982 for RMQ

Most hosting panels provide a firewall page for these rules. Port 22 is often open to any IP by default, but restricting it to your own IP is safer.

## 4. Detect the server in the app

Open Dune Dedicated Server Manager and start a new server setup.

Select Remote Ubuntu over SSH, then provide:

- Server IP
- SSH user
- Private key file

Click Detect remote resources. If the server is configured correctly, the app connects to it and shows the detection result.

## 5. Configure the Dune server

Continue through the setup form:

- Enter your Self-Host Service Token
- Choose the world name and region
- Pick the world layout
- Review the resource checks
- Choose the player-facing IP

The external IP is selected by default when detected.

## 6. Start creation

Click Start Full Setup.

The app provisions the remote server based on your configuration and lets you know when it is ready. It can take some time for the server to appear in-game.
