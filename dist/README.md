# liaisonyy

杨村一中（杨一）内网聊天系统 · liaison + YangYi

> 0.1.0 首个可用版本：注册、登录（PIN/密码）、好友、私聊、班级群、班级/名单管理、重置密钥、审计统计。
> 设计稿见 [设计稿.md](设计稿.md)，使用指南见 [docs/](docs/)。

---

## 1. 目录

```
liaisonyy/
├─ Cargo.toml                 # workspace
├─ build.ps1                  # 设置 MinGW/NASM/Rust 路径后调用 cargo
├─ .cargo/config.toml         # rsproxy 镜像、GNU 链接器、静态 CRT
├─ crates/
│  ├─ core/                   # 协议、Argon2id、X25519 信封加密、AES-GCM
│  ├─ server/                 # axum + SQLite WAL + rustls + 管理后台
│  └─ client/                 # egui 小窗 + 托盘 + 后台网络/加解密线程
├─ admin-web/index.html       # 内嵌网页管理后台
├─ docs/                      # 三份使用指南
├─ 设计稿.md
└─ pmt.md                     # 原始需求（未改动）
```

---

## 2. 构建环境

当前开发机使用：

- Rust 1.99（`x86_64-pc-windows-gnu`）
- 便携 MinGW-w64（WinLibs）
- NASM（rustls/ring 需要）
- 依赖镜像：`rsproxy.cn`（见 `.cargo/config.toml`）

`build.ps1` 会自动把 MinGW、NASM、cargo 加入 PATH。构建命令：

```powershell
# 编译检查
.\build.ps1 check -p liaisonyy-server
.\build.ps1 check -p liaisonyy-client

# 调试构建
.\build.ps1 build -p liaisonyy-server -p liaisonyy-client

# 发布构建（单文件 exe，静态 CRT）
.\build.ps1 build --release -p liaisonyy-server -p liaisonyy-client
```

产物位置：

- `target\debug\liaisonyy-server.exe` / `liaisonyy-client.exe`
- `target\release\...`（发布）

> 构建目录默认在 `liaisonyy\target`；如果磁盘紧张，可在 `.cargo/config.toml` 里把 `target-dir` 改到空间更大的盘。

---

## 3. 运行

### 3.1 服务端

```powershell
mkdir D:\liaisonyy
copy target\debug\liaisonyy-server.exe D:\liaisonyy\
cd D:\liaisonyy
.\liaisonyy-server.exe --data-dir D:\liaisonyy\data --port 47820
```

首次启动会打印 `SETUP TOKEN: ...`。用客户端连接 `https://127.0.0.1:47820` 并初始化。

### 3.2 客户端

```powershell
mkdir D:\liaisonyy-client
copy target\debug\liaisonyy-client.exe D:\liaisonyy-client\
cd D:\liaisonyy-client
.\liaisonyy-client.exe
```

客户端默认连接 `https://127.0.0.1:47820`；可在欢迎页/设置里改服务器地址。

### 3.3 端口

| 端口 | 协议 | 用途 |
|---|---|---|
| 47820 | TCP/TLS | API + 管理后台 |
| 47821 | UDP | 服务发现（0.1.0 未启用） |

管理后台：`https://服务器IP:47820/admin`（自签证书需手动信任）。

---

## 4. 0.1.0 实现状态

| 模块 | 状态 |
|---|---|
| 服务端单文件、SQLite WAL、抗 kill | ✅ |
| 注册/登录（PIN/密码、包含式校验） | ✅ |
| X25519 + AES-256-GCM 信封加密 | ✅ |
| 班级/名单/总管理员初始化 | ✅ |
| 好友申请/接受/拒绝 | ✅ |
| 私聊（纯文本） | ✅ |
| 班级群（纯文本、密钥版本） | ✅ |
| 重置密钥（8 位一次性） | ✅ |
| 管理后台（统计/审计/名单） | ✅ |
| 客户端托盘/右下角小窗/大关闭按钮 | ✅ |
| 离线队列（密文）+ 轮询 | ✅ |
| 转班、举报、头像、自动更新、UDP 发现、WebSocket、老板键、完整软键盘 | ⏳ 后续版本 |
| 管理员聊天解密查看工具 | ⏳ 后续版本（当前只有重置密钥） |

---

## 5. 安全说明

- 消息在客户端用会话密钥加密，服务器只存密文和包裹后的密钥。
- 密码/PIN 用 Argon2id 派生；服务器只存 verify 和长度，不存明文。
- 总管理员/班级管理员托管密钥按设计稿实现（内部机制，不对外披露）。
- 当前版本客户端接受自签 TLS 证书（未做指纹固定），这是 0.1.0 的已知限制。
- 本地缓存只存密文和包裹；退登清除内存密钥。

---

## 6. 已知限制

- 客户端每 2 秒轮询，不是 WebSocket。
- 服务发现、自动更新、老板键、完整软键盘尚未实现。
- 班级群密钥由服务端在成员变化时轮换（服务端短暂接触 GK），1:1 消息为客户端生成 DEK。
- 管理后台与 API 同端口（`0.0.0.0:47820`），没有单独的 loopback 限制；生产环境请用防火墙限制来源，或后续版本拆分端口。
- 没有自动备份；请手动备份 `data\`。

---

## 7. 文档

- [学生使用指南](docs/学生使用指南.md)
- [班级管理员使用指南](docs/班级管理员使用指南.md)
- [总管理员使用指南](docs/总管理员使用指南.md)
- [设计稿](设计稿.md)
