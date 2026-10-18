# liaisonyy

杨村一中（杨一）内网聊天系统 · liaison + YangYi

> 0.2.0：注册/登录、好友/私聊/班群、资料与隐私、改密码/PIN、举报与治理、公告/通知、转班、管理看板/审计/备份/解密、TLS 指纹固定、UDP 发现、签名自动更新、WebSocket、软键盘、23 并发验证。
> 客户端已迁移到 **Tauri 2 + Vue 3 + TypeScript + 纯 CSS**；服务端与 `core` 加密/协议不变。
> 设计稿见 [设计稿.md](设计稿.md)，使用指南见 [docs/](docs/)。

---

## 1. 目录

```
liaisonyy/
├─ Cargo.toml                 # workspace（core / server / client/src-tauri）
├─ build.ps1                  # 本地服务端构建（GNU/MinGW）
├─ .cargo/config.toml         # rsproxy 镜像、GNU 链接器、静态 CRT
├─ crates/
│  ├─ core/                   # 协议、Argon2id、X25519 信封加密、AES-GCM
│  └─ server/                 # axum + SQLite WAL + rustls(ring) + 管理后台
├─ client/
│  ├─ src/                    # Vue 3 + TypeScript 前端
│  ├─ src-tauri/              # Tauri Rust 后端（复用 net/store）
│  ├─ package.json
│  └─ vite.config.ts
├─ admin-web/index.html       # 内嵌网页管理后台（原生 JS）
├─ docs/                      # 三份使用指南
├─ .github/workflows/build.yml
├─ 设计稿.md
└─ pmt.md                     # 原始需求（未改动）
```

---

## 2. 构建

### 2.1 服务端（本地可用）

当前开发机使用 Rust 1.99 GNU + 便携 MinGW + NASM；`build.ps1` 会自动设置 PATH：

```powershell
.\build.ps1 check -p liaisonyy-server
.\build.ps1 build --release -p liaisonyy-server
```

产物：`target\release\liaisonyy-server.exe`（Windows）、CI 另出 Linux musl。

### 2.2 客户端（Tauri，推荐 CI 构建）

Tauri 客户端需要 Windows MSVC；本机磁盘不够装 MSVC，因此官方构建走 **GitHub Actions**：

- `.github/workflows/build.yml`
- `client` job：`windows-latest` + Node 22 + Rust MSVC，`npm ci` → `npm run build:tauri`（`tauri build --no-bundle`）
- `server-windows` job：Windows MSVC 服务端
- `server-linux` job：Linux musl 服务端
- tag `v*` 时自动创建 Release

> 2026-10-18 0.2.0 CI 已通过（run 39102323838）：test 3m5s、server-windows 3m12s、server-linux 1m8s、client 6m30s。客户端 `liaisonyy.exe` 6.13 MB + `WebView2Loader.dll` 0.14 MB；服务端 Windows 5.36 MB、Linux 6.26 MB。`dumpbin` 显示客户端只依赖 Windows 系统 DLL/UCRT，无 `VCRUNTIME140.dll` / `MSVCP140.dll`。

本地只构建前端（无需 MSVC）：

```powershell
cd client
npm install
npm run build
```

> 本地若要跑完整 Tauri，需要安装 Visual Studio Build Tools（C++ 桌面开发），约 7 GB。

### 2.3 产物

- `liaisonyy.exe` + `WebView2Loader.dll`：客户端（Windows x64）
- `liaisonyy-server.exe`：服务端（Windows x64）
- `liaisonyy-server`：服务端（Linux x64，musl 静态）

### 2.4 发布客户端更新

```bash
cargo run -p update-sign -- keygen
cargo run -p update-sign -- sha256 liaisonyy.exe
cargo run -p update-sign -- sign 0.2.0 0.1.0 <sha256> <private>
```

- 构建客户端时设置环境变量 `LIAISONYY_UPDATE_PUBKEY=<public>`（CI 里放 GitHub Secret）。
- 服务端运行时也设置同一个 `LIAISONYY_UPDATE_PUBKEY`，否则拒绝上传更新包。
- 管理后台「发布客户端更新」填写版本号、最低可用版本、签名并选择 exe；客户端下次启动检查并更新。
- 可在「更新包地址」填写 GitHub Releases / Cloudflare Pages 等绝对 URL（留空则用本机 `/api/v1/update/package`）。
- 强制更新：`min_client` 大于客户端当前版本时，客户端必须更新后才能继续。

---

## 3. 运行

### 3.1 服务端

```powershell
mkdir D:\liaisonyy
copy liaisonyy-server.exe D:\liaisonyy\
cd D:\liaisonyy
.\liaisonyy-server.exe --data-dir D:\liaisonyy\data --port 47820
```

首次启动会把初始化口令写入 `D:\liaisonyy\data\setup_token.txt`。Windows 下服务端以**无控制台窗口**方式静默运行，日志写入 `data\logs\server.log`；用客户端连接 `https://127.0.0.1:47820` 并初始化。

也可在 exe 同目录放 `config.toml`：

```toml
school_name = "杨村一中"
server_name = "杨村一中 liaisonyy"
port = 47820
data_dir = "data"
```

命令行 `--data-dir` / `--port` 优先于配置文件；`school_name` / `server_name` 仅在数据库中尚未设置时写入。

### 3.2 客户端

把 `liaisonyy.exe` **和 `WebView2Loader.dll` 放在同一目录**（例如 `D:\liaisonyy-client\`），双击 `liaisonyy.exe`。

- 依赖系统 **WebView2 Runtime**（学校电脑有 Edge/WebView2；缺失时启动会失败）。
- 首次启动显示欢迎页；之后静默进托盘，托盘左键显示/隐藏。
- 大关闭按钮 = 退登 + 隐藏。
- 默认连接 `https://127.0.0.1:47820`；可在欢迎页/设置里改服务器地址。

### 3.3 端口

| 端口 | 协议 | 用途 |
|---|---|---|
| 47820 | TCP/TLS | API + 管理后台 |
| 47821 | UDP | 服务发现 |

管理后台：`https://服务器IP:47820/admin`（自签证书需手动信任；保持原生 JS）。

---

## 4. 0.2.0 实现状态

| 模块 | 状态 |
|---|---|
| 服务端单文件、SQLite WAL、抗 kill | ✅ |
| 注册/登录（PIN/密码、包含式校验） | ✅ |
| 个人资料（签名/简介/性别/生日/社交账号、隐私开关） | ✅ |
| 头像/背景上传（服务端加密存储） | ✅ |
| 修改密码、设置 PIN、登录方式切换 | ✅ |
| 按 YY 号/UID 查找、删除好友、拉黑 | ✅ |
| 举报、隐藏消息、禁言、封禁 | ✅ |
| 公告（全校/班级）、系统通知 | ✅ |
| 转班（进出双向审批、班群密钥轮换） | ✅ |
| 管理看板（在线/会话/登录失败/逐班统计） | ✅ |
| 审计筛选、哈希链校验、CSV 导出 | ✅ |
| 备份（数据库+媒体+密钥）、启动时恢复 | ✅ |
| 管理员聊天解密查看工具（总管/班管托管） | ✅ |
| TLS 指纹固定（TOFU） | ✅ |
| UDP 服务发现（多服务端阻断） | ✅ |
| 签名自动更新（强制/可选） | ✅ |
| WebSocket 即时提醒（2 秒轮询兜底） | ✅ |
| 单实例、老板键 Ctrl+Shift+H、托盘通知 | ✅ |
| 完整软键盘（字母/数字/符号） | ✅ |
| 本地缓存 DPAPI 加密、原子写入 | ✅ |
| 消息/好友申请/举报限流 | ✅ |
| 23 并发账号压测（184 条消息） | ✅ |
| 服务端 config.toml | ✅ |
| 服务端 Windows 静默运行（无控制台） | ✅ |
| 崩溃恢复测试（硬杀后重启） | ✅ |
| X25519 + AES-256-GCM 信封加密 | ✅ |
| 班级/名单/总管理员初始化 | ✅ |
| 好友申请/接受/拒绝 | ✅ |
| 私聊（纯文本） | ✅ |
| 班级群（纯文本、密钥版本） | ✅ |
| 重置密钥（8 位一次性） | ✅ |
| 管理后台（统计/审计/名单） | ✅ |
| Tauri 客户端：托盘/右下角小窗/大关闭按钮 | ✅（CI 已验证；4.01 MB + WebView2Loader.dll） |
| 离线队列（密文）+ 轮询 | ✅ |
| 服务端系统托盘、内置中文输入法 | ⏳ 后续版本 |

---

## 5. 安全说明

- 消息在客户端用会话密钥加密，服务器只存密文和包裹后的密钥。
- 密码/PIN 用 Argon2id 派生；服务器只存 verify 和长度，不存明文。
- 总管理员/班级管理员托管密钥按设计稿实现（内部机制，不对外披露）。
- 客户端使用自签 TLS + **TOFU 指纹固定**：首次连接记录证书 SHA-256，之后指纹变化会拒绝连接；换服务端证书需由管理员在客户端设置里清除对应指纹后重连。
- **自动更新**：清单用 Ed25519 签名、更新包校验 SHA-256；构建客户端时设置 `LIAISONYY_UPDATE_PUBKEY`（base64 公钥），服务端运行时也设置同一公钥才允许上传更新包。签名工具见 `crates/update-sign`。
- **网络时间**：服务端启动/每小时从 NTP 或 HTTP Date 同步；客户端使用服务端时间 + 单调锚点，修改本机墙钟不影响消息时间和重置密钥剩余时间。
- 本地缓存只存密文和包裹；退登清除内存密钥。
- 客户端依赖 WebView2；Tauri 的 webview 不接触密钥，网络/加解密在 Rust 侧。

---

## 6. 已知限制

- 客户端以 WebSocket 即时提醒为主，2 秒轮询作为兜底与断线恢复。
- 服务端没有系统托盘；开机自启可用 `--install-autostart` 写入启动文件夹（C 盘还原环境下可能失效）。
- 内置中文输入法尚未实现；系统输入法仍可使用。
- 自动更新仅支持 Windows；需要构建时与服务端同时配置 `LIAISONYY_UPDATE_PUBKEY`。
- TOFU 首次连接仍可能被中间人抢先，之后指纹变化会被拒绝；换证书需管理员手动清除指纹。
- 客户端分发是 `exe + WebView2Loader.dll` 两个文件（用户已接受）。
- Tauri 客户端内存约 100–250 MB；服务端仍 < 40 MB。
- 班级群密钥由服务端在成员变化时轮换（服务端短暂接触 GK），1:1 消息为客户端生成 DEK。
- 管理后台与 API 同端口（`0.0.0.0:47820`），没有单独的 loopback 限制；生产环境请用防火墙限制来源。
- 备份恢复在服务端下次启动时执行，不会热替换正在使用的数据库。

---

## 7. 文档

- [学生使用指南](docs/学生使用指南.md)
- [班级管理员使用指南](docs/班级管理员使用指南.md)
- [总管理员使用指南](docs/总管理员使用指南.md)
- [设计稿](设计稿.md)
