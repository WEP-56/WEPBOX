# WEPBOX

WEPBOX is a Windows desktop proxy client built with Tauri, Rust, and `sing-box`.

The current goal is a practical daily-use test build: import subscriptions, switch proxy nodes, run TUN mode, stay in the system tray, and provide local troubleshooting and maintenance tools.

[中文 README](../README.md)

## Features

- **Dashboard**
  - Start and stop the `sing-box` core.
  - Switch `Rule / Global / Direct` modes.
  - Toggle TUN mode.
  - Show live traffic, cumulative traffic, CPU, memory, logs, and traffic charts.
- **Nodes**
  - Show proxy groups and nodes.
  - Filter by keyword and subscription source.
  - Test delay for a single node, a group, or all nodes.
  - Batch delay tests support deduplication, bounded concurrency, multiple samples, and median results.
  - Switch the selected proxy for a group.
- **Subscriptions**
  - Import remote subscription URLs.
  - Import pasted local node text.
  - Name subscriptions on import and rename imported subscriptions.
  - Supports `sing-box JSON`, `Clash YAML`, and `V2Ray Base64`.
  - Supports `vmess://`, `vless://`, `trojan://`, `ss://`, `hysteria2://`, `hy2://`, and `tuic://`.
- **Rules And TUN**
  - Built-in common rule sets and service groups.
  - Supports rule mode, FakeIP, DNS, and TUN settings.
  - TUN rule mode includes compatibility handling for local domains, DNS cold start, and QUIC fallback.
- **IP Check**
  - Manually check outbound IP information.
  - Show IPv4 details and local connectivity test results.
  - Add custom connectivity test targets.
- **Settings**
  - Light mode and theme colors.
  - Proxy ports, Clash API, TUN, DNS, fake-dns, and fake-ip settings.
  - Remote subscription refresh, scheduled speed tests, and fastest-node selection.
- **Maintenance**
  - Show current `sing-box` version and runtime paths.
  - Scan `SagerNet/sing-box` releases and switch the local core version.
  - Open config, logs, and subscription cache directories.
  - Validate config, clear logs, and export diagnostics.
  - Clear runtime marker, subscription cache, and Windows proxy leftovers.
- **Desktop Integration**
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
docs/          English README and future docs
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
src-tauri/target/release/bundle/nsis/WEPBOX_0.1.1_x64-setup.exe
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

## Disclaimer

WEPBOX is a universal network proxy client interface built for learning, research, and lawful network management purposes.

This project does **not** provide any proxy services, VPN services, servers, subscriptions, or network access resources of any kind.
All connection configurations, proxy nodes, and related resources are provided and managed solely by the end user.

The author and contributors of this project:

- Do not operate any proxy infrastructure or relay services
- Do not provide or distribute third-party network resources
- Do not guarantee availability, reliability, or security of any user-provided configuration
- Are not responsible for how users utilize this software
- Are not liable for any direct or indirect damages caused by the use of this project

Users are solely responsible for ensuring that their use of this software complies with all applicable laws, regulations, and policies in their respective jurisdictions.

This project is provided "AS IS", without warranty of any kind, express or implied, including but not limited to merchantability, fitness for a particular purpose, and noninfringement.

By using this project, you agree that you are using it at your own risk.

## No Built-In Services

WEPBOX does not include any pre-configured servers, subscriptions, or third-party network endpoints by default.

Any third-party configuration imported by users is entirely unrelated to this project and its contributors.
