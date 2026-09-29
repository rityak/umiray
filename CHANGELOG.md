# Changelog

The version comes from `package.json`, is mirrored in `Cargo.toml`, and appears in
the subscription User-Agent as `umiray/<version>`.

## 1.2.0

### Fixes

- Launch a release build through the installed client. A newer build copies itself to the
  installation directory; an older build cannot replace a newer installed version.
- Repair an administrator startup task that still points to another copy of the client.
  Debug builds keep their separate data, process and startup task.

For Windows x64, download `umiray_1.2.0_x64-setup.exe` from this release's assets.
Existing installations can update from within the client.

## 1.1.0

### New

- Added qd as a second engine, with an engine switch in the title bar and dedicated
  connection, source, routing, settings and log views.
- Added qd core download, process management and per-app routing import/export.

### Fixes

- Fixed tray deadlock, flicker and blank tabs.

For Windows x64, download `umiray_1.1.0_x64-setup.exe` from this release's assets.
The built-in updater can install this release from an existing installation.

## 1.0.1

### Fixes and settings

- Fixed the missing app logo in GitHub builds.
- Moved the client version and update controls into the Client section. Core maintenance
  now has its own Mihomo core section.
- Added English titles for built-in rule sets. The English interface uses the YAML
  `title_en` field when present and falls back to `title`. Custom rules and renamed
  rule sets are preserved.

### Data and development

- Stable data now lives in `%LOCALAPPDATA%\umiray`; dev data lives in
  `%LOCALAPPDATA%\umiray-dev`. On first launch, missing files are copied from the old
  directories. The originals are kept, and existing destination files are not overwritten.
- Dev uses separate `umiray-dev.exe` and `mihomo-dev.exe` processes, settings, window,
  autostart entries and firewall rules. Dev builds cannot install stable client updates.
- New dev configurations use proxy port 3091; existing ports are preserved. Stable and
  dev can run together in Proxy mode. System proxy, TUN and firewall policy remain shared
  Windows resources.

### Installation and updates

For Windows x64, download `umiray_1.0.1_x64-setup.exe` from this release's assets.
In an existing 1.0.0 installation, open Settings → Umiray Settings, select
Check for updates and confirm installation. Settings, sources and the device ID
are preserved. The `.sig` file and `latest.json` are used by the built-in updater.

## 1.0.0

First stable release for Windows x64.

### Updates and distribution

- Signed NSIS installer and update manifest published by GitHub Actions on a version tag.
- Background and manual update checks, release notes and confirmed installation with progress.
- Signature verification finishes before VPN stops. Installation refuses to proceed if the
  core, system proxy or kill-switch could not be released. User data is preserved.

### Connection

- Separate power, capture mode and routing controls. Desired and running modes are
  shown independently; mismatches offer a VPN restart.
- Node tiles and tables show latency, address, protocol, source and live traffic.
  Latency methods are configurable and fallback measurements are marked.
- Automatic core crash recovery with three attempts. Client exit also stops the core;
  orphaned processes are cleaned up on the next launch.
- Nodes are checked again after wake or a network change.

### Sources

- Import subscriptions, individual links, WireGuard and AmneziaWG `.conf` files,
  or add nodes manually through a form or code.
- Subscriptions are normalized into node records. Stored overrides survive refreshes.
- A stable device identifier avoids consuming extra subscription device slots.

### Groups and routing

- Visual group editing with live source lists or fixed selections. Groups are shared
  across routes and presets.
- Visual rule editing, routing presets and rollback. Built-in rule sets are stored
  as data in `collections/rules/`.
- Domain route lookup works before connecting.

### Windows

- Scheduled administrator launch without a UAC prompt on every start; the same task
  handles autostart.
- System proxy, TUN and kill-switch. Graceful shutdown preserves the fake-IP map.

### Anti-DPI

- WireGuard handshake obfuscation with junk packets works with vanilla servers.

### Tools

- Resolver races, DNS tampering and leaks, public IP, UDP, SNI, PMTU, speed,
  firewall, proxy, clock and route diagnostics.
- Smart DNS, automatic MTU selection, preflight checks and connection monitoring.

### Window

- Seven dock sections and customizable [rootik](https://github.com/rityak/rootik) appearance.
- Sections fill the window; only their cards scroll.
- Consistent editor toolbar: form/code and document selection on the left, saving on
  the right. Ctrl+S works wherever Save is available.
- Privacy mode hides server addresses and subscription names for screen sharing.
