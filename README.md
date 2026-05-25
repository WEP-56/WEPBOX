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

---

## 中文

WEPBOX 是一款基于 Tauri 构建的 Windows 桌面代理客户端，使用原生前端和 Rust 后端驱动 `sing-box` 内核。

当前目标是提供实用的测试版本：导入订阅、切换代理节点、运行 TUN 模式、驻留系统托盘，并提供本地故障排查的维护工具。

### 功能特性

- **仪表盘**
  - 启动 / 停止 `sing-box` 内核。
  - 切换 `规则 / 全局 / 直连` 模式。
  - 开关 TUN 模式。
  - 显示实时流量、CPU、内存、日志以及图表悬停数值。
- **节点管理**
  - 展示代理组和节点。
  - 按关键词和订阅来源筛选。
  - 对单个节点、单个分组或全部节点测试延迟。
  - 为代理组切换选定节点。
- **订阅管理**
  - 导入远程订阅链接。
  - 导入粘贴的本地节点文本。
  - 支持 `sing-box JSON`、`Clash YAML`、`V2Ray Base64` 格式。
  - 支持 `vmess://`、`vless://`、`trojan://`、`ss://` 协议。
- **设置**
  - 浅色模式和主题颜色。
  - 代理端口、Clash API、TUN、DNS、fake-dns、fake-ip 设置。
  - 远程订阅刷新、定时测速、自动选择最快节点。
- **维护工具**
  - 显示当前 `sing-box` 版本和运行时路径。
  - 扫描 `SagerNet/sing-box` 发行版并切换本地内核版本。
  - 打开配置、日志和订阅缓存目录。
  - 校验配置、清理日志、导出诊断信息。
  - 清除运行时标记、订阅缓存和 Windows 代理残留。
- **桌面集成**
  - 自定义标题栏。
  - 最小化、最大化、关闭和隐藏到托盘。
  - 系统托盘菜单。
  - 开机自启和自动启动内核。
  - Release 构建在显示主窗口前请求管理员权限。

### 项目结构

```text
frontend/      HTML、CSS 和浏览器端 JavaScript
src-tauri/     Tauri/Rust 后端，配置生成、sidecar 管理、打包
image/         设计源文件和应用程序图标
```

`example/` 为本地参考目录，不纳入项目跟踪。

### 开发环境

环境要求：

- Node.js
- Rust stable
- Windows WebView2 Runtime

安装依赖：

```powershell
npm install
```

开发模式运行：

```powershell
npm run dev
```

前端静态预览：

```powershell
python -m http.server 8000 --bind 127.0.0.1
```

访问：

```text
http://127.0.0.1:8000/frontend/index.html
```

### 代码检查

检查前端脚本：

```powershell
Get-ChildItem .\frontend\scripts\*.js | ForEach-Object { node --check $_.FullName }
```

检查 Rust：

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

运行 Rust 测试：

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml
```

### 构建

默认打包目标为 NSIS：

```powershell
npm run build
```

构建输出：

```text
src-tauri/target/release/wepbox.exe
src-tauri/target/release/sing-box.exe
src-tauri/target/release/bundle/nsis/WEPBOX_0.1.0_x64-setup.exe
```

应用和托盘图标：

```text
src-tauri/icons/icon.ico
```

源图标：

```text
image/icon.ico
```

### 注意事项

- `src-tauri/binaries/sing-box-x86_64-pc-windows-msvc.exe` 为打包的 sidecar 源文件。
- TUN 模式需要管理员权限。
- MSI 打包依赖 WiX，当前项目默认使用 NSIS 进行测试发布。
- 本地缓存、运行日志、构建输出和参考项目不纳入版本控制。

---

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

---

## 免责声明

WEPBOX 是一个通用的网络代理客户端界面，仅供学习、研究和合法的网络管理用途。

本项目**不**提供任何类型的代理服务、VPN 服务、服务器、订阅或网络访问资源。  
所有连接配置、代理节点及相关资源均由最终用户自行提供和管理。

本项目的作者和贡献者：

- 不运营任何代理基础设施或中继服务
- 不提供或分发第三方网络资源
- 不保证任何用户提供的配置的可用性、可靠性或安全性
- 不对用户如何使用本软件负责
- 不对因使用本项目造成的任何直接或间接损害承担责任

用户有责任确保其使用本软件的行为符合所在司法管辖区所有适用的法律、法规和政策。

本项目按"现状"提供，不附带任何明示或暗示的担保，包括但不限于适销性、特定用途适用性和不侵权的担保。

使用本项目即表示您同意自行承担使用风险。

---

## No Built-in Services / 无内置服务

WEPBOX does not include any pre-configured servers, subscriptions, or third-party network endpoints by default.

Any third-party configuration imported by users is entirely unrelated to this project and its contributors.

> WEPBOX 默认不包含任何预配置的服务器、订阅或第三方网络端点。用户导入的任何第三方配置均与本项目及其贡献者无关。
