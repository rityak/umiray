# Changelog

The version comes from `package.json`, is mirrored in `Cargo.toml`, and appears in
the subscription User-Agent as `umiray/<version>`.

## 1.4.0

### Changes

- Update rootik 0.6.1, groups and settings cleanup
- V1.4.0_pre

For Windows x64, download `umiray_1.4.0_x64-setup.exe` from this release's assets.
Existing installations can update from within the client.

## 1.3.2

### New

- Ready-made sets "AI services" and "Geo-blocked for Russia": both go through the VPN even
  when the exit is Direct. Existing installations get them once; a deleted set stays deleted.
- A subscription that failed to update in the background now says so with a line in the
  window, and the line stays until the next successful update.
- Uninstalling the client puts the Windows proxy back: the running client is asked to quit
  cleanly before the installer removes it.

### Changes

- On the qd view "+" opens the `qd://` link dialog straight away. Files, the manual form and
  WARP belong to mihomo, and a non-`qd://` link is refused at the field.
- A subscription node keeps its address, port and path in the editor: another address is
  another node.
- Kill switch follows the capture mode: switching TUN to Proxy lifts it, and a renamed TUN
  adapter gets new allowances without unlocking the machine.
- Node names from subscriptions lose commas: a rule can lead to any node.

### Fixes

- A subscription answering with a page instead of nodes ("subscription expired", a Wi-Fi
  portal) no longer wipes the nodes and sends traffic around the VPN.
- Editing a subscription node's address no longer overwrites the neighbouring node, and
  editing one of two nodes behind the same address no longer copies it over the other.
- A second edit of a subscription node no longer erases the first one.
- A subscription deleted while it was updating no longer comes back.
- Picking TUN without administrator rights while connected no longer leaves the browser
  outside the VPN.
- Background updates no longer stop when the clock was ahead at the last update.
- A rule leading to a group or node with a comma in its name is refused instead of silently
  going through `umiray`.
- `no-resolve` after a regex rule no longer becomes its target, and a space after a comma
  inside a regex is kept.
- A comma in a rule value splits it into values instead of shifting the target.
- A new group no longer opens and edits together with the first one.
- Unsaved changes in mihomo Settings and the WireGuard mask survive leaving the section.
- An untouched node no longer offers "Save".
- Exporting settings onto the client's own database is refused.

## 1.3.1

### Changes

- V131

For Windows x64, download `umiray_1.3.1_x64-setup.exe` from this release's assets.
Existing installations can update from within the client.

## 1.3.0

### New

- First-run setup wizard, also available from the title bar: a source, the capture mode
  and the exit, step by step. "Recommended" writes a tuned mihomo config, picks the fastest
  DNS resolvers and the TUN MTU, can block ads, and has a step for options with
  trade-offs: sniffing and open NAT.
- Rule sets: downloaded lists of domains and subnets — antizapret, antifilter, Telegram,
  YouTube and more from the built-in catalog, or any list by address. Each has its own exit
  and priority and is refreshed in the background; a list from GitHub has a button that
  opens its page.
- Routing is one layered document: your rules, then rule sets and ready-made sets, then
  the client's own rules. Routing can be switched off without losing it.
- Cloudflare WARP node in one click. OpenVPN `.ovpn` and usque files can be added as
  nodes, and every mihomo protocol that has a link is supported.
- qd appears only once it is downloaded: a `qd://` link pasted into any add field
  downloads qd and hands it the subscription.
- Geo databases can be updated with a button while the core is running.
- Export settings into one file.

### Changes

- All client data — settings, documents, sources, presets, rule sets and collections —
  lives in one SQLite database, `umiray.db`. The data folder holds only the client, the
  cores, the database and what the cores read. Existing installations move on first
  start; each file is checked against the database before it is removed.
- The client always runs from `%LOCALAPPDATA%\umiray`. A copy started anywhere else
  installs itself there and starts from there; the startup task and autostart point there
  too.
- Sources moved into Connection: one card switches between nodes and sources, and one "+"
  menu adds anything — a link, a subscription, a file, WARP or a `qd://` link.
- `DIRECT` and `AUTO` are the first rows of the node list instead of a separate switch.
- When the MATCH rule in Routing points somewhere other than your choice, Connection marks
  its target, and picking another exit offers to send everything else there instead.
- DNS and sniffing are on in every capture mode, not only in TUN. DNS resolvers are picked
  from a category: no filtering, ad blocking, or any.
- The Tools section is gone: each check runs where its answer is needed.

### Fixes

- Deleting a subscription no longer stops the VPN from starting when a rule pointed at one
  of its nodes.
- With qd, sleep or a network change no longer turns the VPN off for good.
- A newer portable copy asks to close the running client instead of opening the old one.
- Nodes with the same name in two sources can both be selected.
- One broken node no longer removes its whole source from the core.
- Node code edits are saved from either view; links pasted into one line become separate
  nodes; a custom refresh interval shows what was saved.
- The setup wizard can no longer be closed in the middle of a measurement, shows when the
  capture mode could not be set, and no longer overwrites DNS chosen earlier.

For Windows x64, download `umiray_1.3.0_x64-setup.exe` from this release's assets.
Existing installations can update from within the client.

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
