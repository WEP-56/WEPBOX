# WEPBOX

基于 Tauri + 原生前端 + Rust 后端的 `sing-box` Windows 桌面客户端。

当前目标不是做一个花哨 demo，而是做一个可日常测试、可导入订阅、可切换节点、可管理 TUN、可后台常驻、可排障的桌面代理客户端。

## 当前能力

- 导入和管理订阅
  - 支持 `sing-box JSON`
  - 支持 `Clash YAML`
  - 支持 `V2Ray Base64`
  - 支持 `vmess://` `vless://` `trojan://` `ss://`
- 节点页
  - 分组展示
  - 单节点 / 分组 / 整体测速
  - 选择节点
  - 多订阅来源过滤
- 代理核心
  - `Rule / Global / Direct`
  - `TUN` 启停
  - `DNS Guard / fake-dns / fake-ip`
  - Windows 系统代理回收
- 桌面能力
  - 托盘
  - 开机自启
  - 启动后自动拉起内核
  - 关闭窗口隐藏到后台
  - 启动时隐藏主窗口
- 运行维护
  - 配置检查
  - 日志目录打开
  - 诊断信息导出
  - runtime marker 清理
  - 系统代理残留清理

## 目录结构

```text
frontend/    前端页面、脚本、样式
src-tauri/   Tauri Rust 后端、配置生成、sidecar 管理
```

`example/` 中的参考项目不纳入版本控制，只用于本地参考。

## 开发

先安装依赖：

```powershell
npm install
```

开发模式：

```powershell
npm run dev
```

构建：

```powershell
npm run build
```

单独检查 Rust：

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml -- --nocapture
```

检查前端脚本语法：

```powershell
Get-ChildItem .\frontend\scripts\*.js | ForEach-Object { node --check $_.FullName }
```

## 说明

- 仓库当前只提交主项目代码和必要资源，不提交本地测试数据与参考仓库。
