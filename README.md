# WEPBOX

WEPBOX 是一个基于 Tauri、原生前端和 Rust 后端的 `sing-box` Windows 桌面代理客户端。当前目标是先做一版可日常测试的客户端：能导入订阅、切换节点、启停 TUN、后台常驻、托盘控制和执行运行维护。

## 当前能力

- 首页
  - 启停 `sing-box` 内核
  - `Rule / Global / Direct` 模式切换
  - TUN 开关
  - 实时速率、CPU、内存和运行日志展示
- 节点
  - 代理分组展示
  - 节点搜索和订阅来源筛选
  - 单节点、分组和整体测速
  - 切换当前分组选中节点
- 订阅
  - 导入远程订阅链接
  - 导入本地粘贴节点文本
  - 支持 `sing-box JSON`、`Clash YAML`、`V2Ray Base64`
  - 支持 `vmess://`、`vless://`、`trojan://`、`ss://`
- 设置
  - 浅色模式和主题色
  - 代理端口、Clash API、TUN、DNS、fake-dns、fake-ip 配置
  - 自动更新订阅、计划测速、自动选择最快节点
- 运行维护
  - 查看当前内核版本和运行路径
  - 从 `SagerNet/sing-box` 发布页扫描并切换内核版本
  - 打开配置、日志、订阅缓存目录
  - 配置检查、日志清理、诊断导出
  - 清理 runtime marker、订阅缓存和系统代理残留
- 桌面能力
  - 自定义标题栏
  - 最小化、最大化、关闭、隐藏到后台
  - 系统托盘菜单
  - 开机自启和启动后自动拉起内核

## 目录结构

```text
frontend/      原生前端页面、脚本和样式
src-tauri/     Tauri/Rust 后端、配置生成、sidecar 管理和打包配置
image/         设计资源和应用图标源文件
```

`example/` 是本地参考项目目录，不纳入版本控制。

## 开发环境

需要先安装 Node.js、Rust stable、Windows WebView2 Runtime。

安装前端依赖：

```powershell
npm install
```

启动开发环境：

```powershell
npm run dev
```

前端静态预览：

```powershell
python -m http.server 8000 --bind 127.0.0.1
```

然后打开：

```text
http://127.0.0.1:8000/frontend/index.html
```

## 检查

检查前端脚本语法：

```powershell
Get-ChildItem .\frontend\scripts\*.js | ForEach-Object { node --check $_.FullName }
```

检查 Rust 后端：

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

运行 Rust 测试：

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml
```

## 打包

当前默认生成 NSIS 安装包：

```powershell
npm run build
```

主要产物：

```text
src-tauri/target/release/simple-singbox-client.exe
src-tauri/target/release/sing-box.exe
src-tauri/target/release/bundle/nsis/代理客户端_0.1.0_x64-setup.exe
```

应用和托盘图标使用：

```text
src-tauri/icons/icon.ico
```

源图标来自：

```text
image/icon.ico
```

## 注意事项

- `src-tauri/binaries/sing-box-x86_64-pc-windows-msvc.exe` 是打包时使用的 sidecar。
- TUN 模式需要管理员权限。
- MSI 打包依赖 WiX。本项目当前默认使用 NSIS，适合先做非生产环境冒烟测试。
- 本项目暂不提交本地缓存、运行日志、构建产物和参考项目。
