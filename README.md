<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" alt="umiray">
</p>

<h1 align="center">Umiray</h1>

<p align="center">
  A proxy client for Windows and Linux powered by <a href="https://github.com/MetaCubeX/mihomo">Mihomo</a>.<br>
  Tauri&nbsp;2 · Rust · React · <a href="https://github.com/rityak/rootik">Rootik</a>
</p>

<p align="center"><b>1.6.1</b> · <a href="README.ru.md">Русская версия</a></p>

> **Umiray is a proxy client for Windows and Linux powered by Mihomo.** It runs solely on the user’s computer, at their request, and provides local proxy, system proxy or TUN modes. It includes no servers, does not encrypt or tunnel traffic by itself, and grants no access to any service. The code and builds are provided as is. You are responsible for using them in accordance with the laws of your country and the terms of your network provider. The authors do not encourage any violation of these rules and accept no liability for any use.


![Connection](screenshots/connection.png)

## Features

- One-click connection with separate Proxy, System and TUN modes.
- A setup wizard on first launch: core settings, subscription, capture mode and route.
- Direct, Auto and Manual routing, or your own rules.
- Subscriptions, individual links, WireGuard and AmneziaWG files, and custom nodes.
  Node edits survive subscription refreshes.
- Groups and routing rules editable as forms or YAML, with presets and rollback.
- DNS, MTU, clock, leak and speed diagnostics.
- Kill-switch, autostart and scheduled administrator launch.
- Privacy mode hides server addresses and subscription names for screenshots and streams.
- Signed client updates from GitHub Releases, with confirmation and download progress.

## Installation and first launch

Download from [Releases](https://github.com/rityak/umiray/releases/latest):

- **Windows x64** — `umiray_<version>_x64-setup.exe`.
- **Debian, Ubuntu, Mint** (Debian 12+, Ubuntu 22.04+) — `umiray_<version>_amd64.deb`:
  `sudo apt install ./umiray_<version>_amd64.deb`.
- **Fedora** and other rpm systems — `umiray-<version>-1.x86_64.rpm`:
  `sudo dnf install ./umiray-<version>-1.x86_64.rpm`.
- **Arch** and derivatives — `umiray-bin` from the AUR (`yay -S umiray-bin`), or the `PKGBUILD`
  and `umiray.install` from the release: `makepkg -si`.

The client downloads mihomo from its official release page on first launch.
Add your subscription, proxy link or configuration file, choose a node and press Connect.
Proxy mode provides a local proxy for applications; System enables the system proxy;
TUN captures traffic through a virtual interface. Servers are not included.

TUN needs extra rights. On Windows, the client can create a scheduled task to launch with
administrator rights without asking for UAC confirmation each time. On Linux, the client
runs as you and gives only the core the network rights through polkit; in your desktop
session no password is asked.

Data is stored in `%LOCALAPPDATA%\umiray` on Windows and `~/.local/share/umiray` on Linux.

The interface selects Russian when a Russian keyboard layout is installed; otherwise,
it selects English. Override this in Settings → Umiray Settings → Interface language.

## Umiray Core / VOLT

[umiray-core](https://github.com/rityak/umiray-core) contains the optional Windows traffic
transformer VOLT. It changes TCP segmentation and packet order and can add decoy packets
while preserving the real data stream.

## Dependencies and build

Requires Node.js 22+, Rust stable and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/):
Microsoft C++ Build Tools and WebView2 on Windows; WebKitGTK 4.1 and the listed development
packages on Linux. JavaScript and Rust dependencies are installed by npm and Cargo.

```sh
npm ci
npm run tauri dev
```

Build an executable without an installer:

```sh
npm run tauri build -- --no-bundle
```

The executable is `src-tauri/target/release/umiray.exe` (`umiray` on Linux). Linux packages:
`npx tauri build --bundles deb,rpm`; build them on Ubuntu 22.04 so they run on older systems too.

Checks: `npm run lint`, `npm test`, `npx tsc --noEmit`, and `cargo test` in `src-tauri`.

`npm run dev` opens the interface in a browser with demo data.

Report bugs in [Issues](https://github.com/rityak/umiray/issues).
Code and builds are provided as is; users are responsible for complying with applicable
laws and provider terms. The authors accept no liability for use.
