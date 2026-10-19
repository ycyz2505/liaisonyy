# liaisonyy

杨村一中（杨一）内网聊天系统 · liaison + YangYi

> 0.3.2：自动更新改由 **GitHub Releases** 提供（清单/客户端/服务端），内置多个 GitHub 加速源；客户端每次启动强制检查更新，更新成功才能进入；小窗不再自动出现，只能点托盘图标打开，且随时可关闭；服务端也会在启动时自动检查并更新自己，更新状态写在程序同目录的 `update-status.txt`。自带中文键盘仍保持下线，文本输入使用系统输入法；PIN/密码页面键盘保留。
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
│  ├─ server/                 # axum + SQLite WAL + rustls(ring) + 管理后台
│  └─ update-sign/            # 旧版 Ed25519 签名工具（当前 Releases 流程不使用）
├─ client/
│  ├─ src/                    # Vue 3 + TypeScript 前端
│  ├─ src-tauri/              # Tauri Rust 后端（复用 net/store）
│  ├─ package.json
│  └─ vite.config.ts
├─ admin-web/index.html       # 内嵌网页管理后台（原生 JS）
├─ docs/                      # 三份使用指南
├─ .github/workflows/build.yml
├─ 设计稿.md
└─ pmt.md                     # 原始需求与补充要求
```

---

## 2. 构建

### 2.1 服务端（本地可用）

当前开发机使用 Rust 1.99 GNU + 便携 MinGW + NASM；`build.ps1` 会自动设置 PATH：

```powershell
.\build.ps1 check -p liaisonyy-server
.\build.ps1 build --release -p liaisonyy-server
```

产物：`target\release\liaisonyy-server.exe`（Windows）。

### 2.2 客户端（Tauri，推荐 CI 构建）

Tauri 客户端需要 Windows MSVC；本机磁盘不够装 MSVC，因此官方构建走 **GitHub Actions**：

- `.github/workflows/build.yml`
- `client` job：`windows-latest` + Node 22 + Rust MSVC，`npm ci` → `npm run build:tauri`（`tauri build --no-bundle`）
- `server-windows` job：Windows MSVC 服务端
- tag `v*` 时自动创建 Release

> 0.3.2 的 CI 已通过（run 39274463205）：test 4m20s、server-windows 4m43s、client 8m55s。该次运行仍包含 Linux job；从这次起后续构建已移除 Linux，只保留 Windows。

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

> 0.3.2 起发布构建只做 Windows；Linux 服务端源码仍保留，但不再由 CI 构建或发布。

### 2.4 发布自动更新（GitHub Releases）

版本清单与安装包统一放在 GitHub Releases：

- 清单：`https://github.com/ycyz2505/liaisonyy/releases/latest/download/manifest.json`
- 客户端：`.../releases/download/v<版本>/liaisonyy.exe`
- 服务端 Windows：`.../releases/download/v<版本>/liaisonyy-server.exe`

发布新版本：

- 打 tag：`git tag v0.3.3 && git push origin v0.3.3`
- 或 Actions → build → Run workflow，勾选 `publish_update`，填 `version` / `min_client`

`publish-update` 工作流会构建客户端和 Windows 服务端、计算 SHA-256、生成 `manifest.json`（含 `files.windows` 与 `server.windows`），并创建/更新 GitHub Release。

- 客户端每次启动都会强制检查清单：有更新必须更新成功才能进入；检查失败只能重试，不能跳过。下载会并发尝试直连和内置加速源（`ghfast.top`、`gh-proxy.com`、`ghproxy.net`），校验 SHA-256 后自替换重启。
- 服务端启动时也会检查同一份清单：发现新版本会下载、校验并自替换重启；没有命令行/UI，状态写入程序同目录的 `update-status.txt`（不可写时退回数据目录），详细日志在 `data/logs/server.log`。服务端更新失败不会阻止本次启动，下次启动会再试。
- 更新过程的小窗会显示当前阶段；小窗绝不自动弹出，只能点托盘图标（或按 `Ctrl+Shift+H` / 托盘菜单）打开，关闭按钮随时可用。
- 0.3.1 及更早客户端没有内置新更新逻辑，需要手动安装一次 0.3.2+。
- 完整步骤见 [docs/GitHub-Releases自动更新指南.md](docs/GitHub-Releases自动更新指南.md)。

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

### 3.4 输入法与 QQ 群

- 聊天、资料、公告等文本输入使用**系统输入法**（微软拼音/搜狗等）。
- PIN/密码页面的自带键盘保留：PIN 数字键盘、密码 QWERTY/符号键盘。
- 自带中文键盘（26/9 键）因 bug 暂时下线，后续修复后再提供；当前不再加载 my_rime/WASM 资源。
- liaisonyy 用户 QQ 群：**1130941127**。登录后「我」页可一键复制群号；欢迎反馈 bug、获取更新通知。

---

## 4. 0.3.2 实现状态

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
| 自动更新（GitHub Releases，客户端+服务端，内置加速源） | ✅ |
| WebSocket 即时提醒（2 秒轮询兜底） | ✅ |
| 单实例、老板键 Ctrl+Shift+H、托盘通知 | ✅ |
| PIN/密码页面自带键盘（中文键盘暂时下线） | ✅ |
| 自适应窗口大小（按屏幕分辨率比例） | ✅ |
| liaisonyy QQ 群 1130941127 | ✅ |
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
| Tauri 客户端：托盘/右下角小窗/大关闭按钮 | ✅（CI 构建；`liaisonyy.exe + WebView2Loader.dll`） |
| 离线队列（密文）+ 轮询 | ✅ |
| 服务端系统托盘 | ⏳ 后续版本 |

---

## 5. 安全说明

- 消息在客户端用会话密钥加密，服务器只存密文和包裹后的密钥。
- 密码/PIN 用 Argon2id 派生；服务器只存 verify 和长度，不存明文。
- 总管理员/班级管理员托管密钥按设计稿实现（内部机制，不对外披露）。
- 客户端使用自签 TLS + **TOFU 指纹固定**：首次连接记录证书 SHA-256，之后指纹变化会拒绝连接；换服务端证书需由管理员在客户端设置里清除对应指纹后重连。
- 文本输入使用系统输入法；PIN/密码页面的自带键盘只处理密码/PIN，不加载外部输入法资源。
- **自动更新**：清单与安装包来自 GitHub Releases，下载后校验清单中的 SHA-256；客户端每次启动强制检查，检查失败只能重试；服务端启动时也会自检自更新。下载会并发尝试直连与内置加速源，避免单一源被墙导致卡死。0.3.1 及更早客户端需手动安装一次 0.3.2+。
- **网络时间**：服务端启动/每小时从 NTP 或 HTTP Date 同步；客户端使用服务端时间 + 单调锚点，修改本机墙钟不影响消息时间和重置密钥剩余时间。
- 本地缓存只存密文和包裹；退登清除内存密钥。
- 客户端依赖 WebView2；Tauri 的 webview 不接触密钥，网络/加解密在 Rust 侧。

---

## 6. 已知限制

- 客户端以 WebSocket 即时提醒为主，2 秒轮询作为兜底与断线恢复。
- 服务端没有系统托盘；开机自启可用 `--install-autostart` 写入启动文件夹（C 盘还原环境下可能失效）。
- 自带中文键盘（26/9 键）因 bug 暂时下线；文本输入使用系统输入法，PIN/密码页面键盘保留。
- 小窗大小按当前屏幕逻辑高度 / 1.7、宽度 = 高度 / 1.79 自适应（高度限制 420–700 逻辑像素），低分辨率或高缩放下会自动缩小。
- liaisonyy 用户 QQ 群：1130941127（登录后「我」页可复制）。
- 自动更新需要能访问 GitHub Releases 或内置加速源；客户端启动时若所有源都不可达，会停在「检查更新失败」界面只能重试（这是「必须检查更新」的强制行为）。服务端更新失败只写状态文件并继续用旧版，下次启动再试。
- 服务端没有命令行/UI，更新状态见程序同目录 `update-status.txt`，日志见 `data/logs/server.log`。
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
- [GitHub Releases 自动更新指南](docs/GitHub-Releases自动更新指南.md)
- [设计稿](设计稿.md)
