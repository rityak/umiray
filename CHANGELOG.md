# Changelog

The version comes from `package.json`, is mirrored in `Cargo.toml`, and appears in
the subscription User-Agent as `umiray/<version>`.

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
