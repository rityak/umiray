# qd inside umiray

umiray can drive two engines: **mihomo** (as before) and **qd** (github.com/jaywehosl/qd).
The engine switch in the title bar keeps the same tabs and swaps what is inside them. The backend is swapped
when the power button is pressed: turning qd on stops mihomo first (its system proxy and kill
switch go with it), turning mihomo on stops qd first. They never capture traffic at the same time.

## What each tab becomes in qd mode

| Tab | qd mode |
|---|---|
| Connection | same layout: power card (capture mode and route are hidden — qd captures per app and picks the entry node itself; +egress and ad blocking take their place), the same traffic chart, the same node list with qd entry nodes (no editing, no latency method) |
| Sources | same layout: add a `qd://` link, subscription refresh interval, the subscription as a source card (refresh, delete = unlink, expiry and traffic) |
| Groups | hidden: qd picks entry nodes itself |
| Routing | same look as the mihomo rules: summary, numbered rule rows (app → role), MATCH at the bottom holds the default role; a new rule is picked in a dialog of running apps (grouped by path, searchable, already-ruled ones marked) or typed by process name; save/load `.qdr` through the Windows file dialogs |
| Settings | the client document keeps only what applies to both engines (startup, window, client update, reset); the mihomo document is replaced by **qd Settings**: fixed send rate, update the qd binary |
| Tools | unchanged |
| Logs | qd output |

## The qd process

- Needs qd 0.1.4 or newer: the first release with `-embedded` and the core build.
- Binary: `%LOCALAPPDATA%\umiray\qd.exe` (`qd-dev.exe` in debug builds). **Download qd** fetches
  `qd-core-windows-amd64.exe` from the newest release of github.com/jaywehosl/qd and checks it
  against the release's `checksums.txt`.
  The core build has no window, tray, page or admin panel: only the tunnel and the local API.
- Needs administrator rights: qd captures traffic with WinDivert, and its manifest asks for elevation.
  Without them qd mode shows the usual "restart as administrator" action.
- Started lazily, the first time a qd tab needs it, inside the same job object as mihomo:

  ```
  qd.exe -embedded -ui-port 0 -state %LOCALAPPDATA%\umiray\qd\client.db
  ```

- `-embedded`: no window, no tray, log to stdout, stop cleanly when stdin closes. The first
  stdout line that starts with `{"qdEmbedded"` is the handshake:

  ```json
  {"qdEmbedded":{"api":"http://127.0.0.1:PORT","token":"…"}}
  ```

  or `{"qdEmbedded":{"error":"…"}}` when it cannot start (another qd client already runs on the machine).
- Every request carries `X-QD-Token: <token>`. Requests with a foreign `Origin` are refused, so the
  window never talks to qd directly — everything goes through the `qd_*` Tauri commands.
- Stop: umiray closes qd's stdin; after 5 s it kills the process.

## API used (all under `/client/api/`, JSON `{success, msg, obj}`)

| Call | Purpose |
|---|---|
| `GET state` | connected, node, nodes count, egress, adblock, allowExit, subscription |
| `POST connect` / `POST disconnect` | bring the tunnel up / down |
| `POST toggle {egress?, adblock?}` | +egress and ad blocking |
| `GET nodes` | entry nodes with reachability and latency |
| `POST import {uri}` / `POST reset {subscription:true}` | take / drop the `qd://` link |
| `POST subscription/refresh`, `GET about` | refresh now; name, expiry, traffic |
| `GET history/1` | per-second up/down for the last minute |
| `GET/POST routing`, `GET routing/processes` | per-app rules, running apps with icons |
| `GET routing/export`, `POST routing/import {code}` | `.qdr` files |
| `GET/POST settings` | `refreshMinutes`, `fixedRate` (patch semantics) |

## Code

- `src-tauri/src/core/qd.rs` — process, handshake, API proxy, download.
- `src-tauri/src/commands/qd.rs` — `qd_status`, `qd_call`, `qd_start`, `qd_stop`, `qd_install`, `qd_logs`,
  `qd_rules_export`, `qd_rules_import` (native save/open dialogs from `src-tauri/src/system/pick.rs`).
- `src-tauri/src/app/connect.rs` — mihomo start stops qd; tray toggle and autoconnect follow the engine.
- `src/qd/*` — the qd tabs; `src/App.tsx` and `src/chrome/TitleBar.tsx` — the switch.
