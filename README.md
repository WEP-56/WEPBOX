# WEPBOX

WEPBOX is a Windows desktop proxy client built with Tauri, a native frontend, and a Rust backend around `sing-box`.

The current goal is a practical test build: import subscriptions, switch proxy nodes, run TUN, stay in tray, and provide maintenance tools for local troubleshooting.

## Features

- Dashboard
  - Start and stop the `sing-box` core.
  - Switch `Rule / Global / Direct` modes.
  - Toggle TUN mode.
  - Show live traffic, CPU, memory, logs, and chart hover values.
- Nodes
  - Show proxy groups and nodes.
  - Filter by keyword and subscription source.
  - Test delay for a node, a group, or all nodes.
  - Switch selected proxy for a group.
- Subscriptions
  - Import remote subscription URLs.
  - Import pasted local node text.
  - Supports `sing-box JSON`, `Clash YAML`, `V2Ray Base64`.
  - Supports `vmess://`, `vless://`, `trojan://`, `ss://`.
- Settings
  - Light mode and theme colors.
  - Proxy ports, Clash API, TUN, DNS, fake-dns, fake-ip settings.
  - Remote subscription refresh, scheduled speed tests, fastest-node selection.
- Maintenance
  - Show current `sing-box` version and runtime paths.
  - Scan `SagerNet/sing-box` releases and switch the local core version.
  - Open config, logs, and subscription cache directories.
  - Validate config, clear logs, export diagnostics.
  - Clear runtime marker, subscription cache, and Windows proxy leftovers.
- Desktop integration
  - Custom title bar.
  - Minimize, maximize, close, and hide to tray.
  - System tray menu.
  - Auto launch and auto start core.
  - Release builds request administrator permission before the main window opens.

## Structure

```text
frontend/      HTML, CSS, and browser-side JavaScript
src-tauri/     Tauri/Rust backend, config generation, sidecar management, packaging
image/         Source design assets and app icon
```

`example/` is a local reference directory and is not part of the tracked project.

## Development

Requirements:

- Node.js
- Rust stable
- Windows WebView2 Runtime

Install dependencies:

```powershell
npm install
```

Run in development mode:

```powershell
npm run dev
```

Static frontend preview:

```powershell
python -m http.server 8000 --bind 127.0.0.1
```

Open:

```text
http://127.0.0.1:8000/frontend/index.html
```

## Checks

Check frontend scripts:

```powershell
Get-ChildItem .\frontend\scripts\*.js | ForEach-Object { node --check $_.FullName }
```

Check Rust:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Run Rust tests:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml
```

## Build

The default bundle target is NSIS:

```powershell
npm run build
```

Release outputs:

```text
src-tauri/target/release/wepbox.exe
src-tauri/target/release/sing-box.exe
src-tauri/target/release/bundle/nsis/WEPBOX_0.1.0_x64-setup.exe
```

App and tray icon:

```text
src-tauri/icons/icon.ico
```

Source icon:

```text
image/icon.ico
```

## Notes

- `src-tauri/binaries/sing-box-x86_64-pc-windows-msvc.exe` is the packaged sidecar source.
- TUN mode requires administrator permission.
- MSI bundling depends on WiX. This project currently defaults to NSIS for test releases.
- Local cache, runtime logs, build outputs, and reference projects are not committed.
