# Changelog

本项目遵循「每次改动同步更新文档/设计稿/更新日志」的约定。

## v0.2.0（开发中）

### 安全与隐私

- **注册表单残留修复**：每次进入「注册新账号」清空上一位同学的姓名选择、YY 号、用户名、密码、PIN、登录策略；登录/退登时清空注册、登录、重置、Setup、聊天输入、好友附言等敏感表单。
- **Setup 恢复码流程修复**：初始化完成后停留在恢复码页，必须点「我已保存，去登录」才离开；支持复制恢复码。
- **全链路网络时间**：
  - 服务端启动时及每小时通过 NTP（time.cloudflare.com / ntp.aliyun.com / pool.ntp.org）或 HTTP Date（百度 / Cloudflare / GitHub）同步权威时间；失败时回退本地时钟并告警。
  - 服务端所有时间戳（消息、审计、重置密钥过期、会话）使用网络时间锚点 + 单调时钟，学生改本地时间不影响。
  - 客户端从服务端 `server_time` 初始化核心时钟；UI 用 `performance.now()` 单调锚点，消息时间、重置密钥剩余时间、列表时间都不依赖本机墙钟。
- 服务端 e2e 在 ring TLS + 网络时间下通过（11.47s）。

### 文档

- 设计稿升级到 v0.7（进行中），补充网络时间与敏感表单清理。
- 本 CHANGELOG 新增。

## v0.1.0（2026-10-18）

- 首个可用版本：注册/登录（PIN/密码、包含式、Argon2id）、好友申请、私聊、班级群、重置密钥、基础管理（建班/导名单/设班管/统计/审计）。
- 客户端从 egui 迁移到 Tauri 2 + Vue 3 + TypeScript + 纯 CSS；服务端 Rust/axum/SQLite/rustls(ring)。
- GitHub Actions：Windows MSVC 客户端/服务端 + Linux musl 服务端；客户端 `liaisonyy.exe` + `WebView2Loader.dll`。
- 交付包：`liaisonyy-0.1.0-win64-tauri.zip`。
