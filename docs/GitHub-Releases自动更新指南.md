# GitHub Releases 自动更新（客户端 + 服务端）

liaisonyy 的版本清单和安装包主要放在 **GitHub Releases**，同时支持**服务端本地更新源**（`data/update/`）作为兜底：

- 仓库：`https://github.com/ycyz2505/liaisonyy`
- 最新清单：`https://github.com/ycyz2505/liaisonyy/releases/latest/download/manifest.json`
- 客户端：`.../releases/download/v<版本>/liaisonyy.exe`
- 服务端 Windows：`.../releases/download/v<版本>/liaisonyy-server.exe`

每次发布只需要创建/更新一个 Release，上传上面三个文件；客户端和服务端会同时检查 GitHub 和服务端本地源，选版本号最高的那个更新。0.3.2 起发布构建只做 Windows，Linux 服务端源码保留但不再构建/发布。

---

## 1. 自动发布（GitHub Actions）

`.github/workflows/build.yml` 里的 `publish-update` job 会：

1. 等待 `client`、`server-windows` 两个构建 job；
2. 收集 `liaisonyy.exe`、`liaisonyy-server.exe`；
3. 分别计算 SHA-256（base64）；
4. 生成 `manifest.json`（含客户端与服务端条目）；
5. 创建/更新 GitHub Release（tag `v<version>`），上传三个文件。

触发方式：

### 方式 A：打 tag

```bash
git tag v0.3.3
git push origin v0.3.3
```

### 方式 B：手动运行

GitHub → **Actions → build → Run workflow**：

- `publish_update` 勾选
- `version` 填新版本，例如 `0.3.3`
- `min_client` 填最低可用版本，例如 `0.3.0`

工作流会自动创建 `v0.3.3` Release 并上传三个文件。

> 不需要配置任何签名私钥；清单里的 SHA-256 和 GitHub 的 HTTPS 是当前完整性校验。以后如果需要 Ed25519 签名，可再启用仓库里的 `update-sign` 工具。

---

## 2. 清单格式

```json
{
  "schema": 1,
  "version": "0.3.3",
  "min_client": "0.3.0",
  "files": {
    "windows": {
      "url": "https://github.com/ycyz2505/liaisonyy/releases/download/v0.3.3/liaisonyy.exe",
      "sha256": "<客户端 SHA-256 的 base64>",
      "size": 6130688
    }
  },
  "server": {
    "windows": {
      "url": "https://github.com/ycyz2505/liaisonyy/releases/download/v0.3.3/liaisonyy-server.exe",
      "sha256": "<Windows 服务端 SHA-256 的 base64>",
      "size": 5620000
    }
  }
}
```

- 客户端只读 `files.windows`；Windows 服务端读 `server.windows`。
- 下载后都会用清单里的 `sha256` 校验，不匹配就丢弃。
- `min_client` 保留在清单里；当前客户端策略是「只要清单版本高于当前版本就必须更新」，不区分是否强制。

---

## 3. GitHub 加速（已内置）

国内直连 GitHub 可能很慢或失败。客户端和服务端内置了以下加速前缀（只是 URL 前缀，体积极小，不额外打包二进制）：

1. 直连：`""`
2. `https://ghfast.top/`
3. `https://gh-proxy.com/`
4. `https://ghproxy.net/`

使用方式是把原始 GitHub URL 拼在前缀后面，例如：

```
https://ghfast.top/https://github.com/ycyz2505/liaisonyy/releases/latest/download/manifest.json
```

客户端会**并发**尝试直连和各加速前缀，谁先返回有效清单/通过校验的安装包就用谁；某个源失败或 SHA-256 不匹配会被自动忽略。这样即使直连被墙挂起，加速源也能先完成。

可以手动测试哪个加速可用：

```bash
curl -I -L --max-time 15 "https://ghfast.top/https://github.com/ycyz2505/liaisonyy/releases/latest/download/manifest.json"
```

把可用的前缀留在 `client/src-tauri/src/net.rs` 的 `GITHUB_ACCELERATORS` 与 `crates/server/src/main.rs` 的同名常量里即可。

---

## 4. 客户端行为

- **两个更新源**：客户端同时查询 GitHub Releases（直连 + 加速源）和当前服务端 `/api/v1/update/manifest`；在所有能连上的源里选**版本号最高**的那个，再从该源下载。以后新增源也走同一套逻辑。
- **每次启动强制检查**：有更新时必须更新成功才能进入，检查失败只能「重试」，不能跳过。
- **更新 UI**：小窗显示「正在检查更新 / 正在下载更新 / 更新包校验通过，正在安装 / 更新已就绪，正在重启」等阶段。
- **不自动弹窗**：小窗启动时保持隐藏，只能点托盘图标（或 `Ctrl+Shift+H`、托盘菜单）打开；界面始终有「关闭窗口」按钮，随时可隐藏，隐藏后更新检查仍在后台继续。
- **服务端源**：服务端会把 GitHub 清单和本地 `data/update/manifest.json` 中版本更高的有效清单返回给客户端，并把包地址改写为 `/api/v1/update/package?source=github|local`，由服务端中转下载。GitHub 完全不可达时，只要服务端本地有清单和包，客户端仍能更新。
- **自替换**：下载并校验后写 `liaisonyy-new.exe`，用隐藏批处理等待进程退出、覆盖旧 exe、启动新 exe。

## 5. 服务端行为

- Windows 服务端启动时（日志初始化后、开始服务前）同时检查 **GitHub Releases** 和 **本地 `data/update/manifest.json`**，在能连上的源里选版本号最高、且带 `server.windows` 的清单；发现新版本后下载、校验 SHA-256，然后自替换并重启：写 `liaisonyy-server-new.exe` + 隐藏批处理，退出旧进程，批处理覆盖 exe 并用原参数启动。
- 本地服务端包约定放在 `data/update/server.exe`（Linux 为 `data/update/server`）；本地清单里的 `server.windows.url` 可以是相对路径，也可以是绝对 HTTP(S) URL。
- Linux 服务端源码保留同样的自更新逻辑，但 0.3.2 起发布清单不再提供 Linux 包，因此 Linux 版需要自行编译/手动升级。
- **没有命令行/UI**：更新状态写入程序同目录的 `update-status.txt`（同目录不可写时退回数据目录），会注明来源，例如：

  ```
  liaisonyy 服务端更新状态
  更新时间: 2026-10-20 03:21:45 UTC
  当前版本: 0.3.2
  状态: 正在下载更新
  说明: 发现新版本 0.3.3（来源：GitHub Releases），正在下载服务端…
  ```

  详细日志仍在 `data/logs/server.log`。
- 更新失败不会阻止服务端本次启动；它会继续用旧版本服务，并在下次启动时再试。
- `/api/v1/update/manifest` 与 `/api/v1/update/package` 对老客户端仍然可用：清单会选 GitHub/本地中版本最高的有效项，包接口支持 `?source=github|local`，不带参数时保持旧行为（GitHub 优先、本地兜底）。
- 测试时设置环境变量 `LIAISONYY_NO_SELF_UPDATE=1` 可禁止服务端自更新；设置 `LIAISONYY_UPDATE_OFFLINE=1` 可让服务端清单/安装包接口跳过 GitHub，只用本地 `data/update/`（仓库内的自动化测试已默认设置这两个变量）。

### 5.1 本地更新源（GitHub 不可达时）

- 客户端：`data/update/manifest.json` + `data/update/package.exe`。管理后台「发布客户端更新」会写这两个文件；也可以手动放入。
- 服务端自更新：在 `data/update/manifest.json` 里加 `server.windows`，包放 `data/update/server.exe`。
- 清单里的 `version` 必须高于当前版本才会触发更新；`sha256` 必须与实际文件一致，否则会被丢弃。
- 本地源和 GitHub 源同时可用时，客户端/服务端会选版本号更高的那个。

---

## 6. 首次上线注意

- 0.3.1 及更早客户端没有 GitHub 更新逻辑，需要手动安装一次 0.3.2 或更高版本；之后即可自动更新。
- 仓库必须**公开**（或使用一个公开的 releases-only 仓库）。私有仓库的 Release 资产对未登录请求返回 404，客户端和服务端都无法自动更新。`ycyz2505/liaisonyy` 已设为公开。
- Release 必须是 **非预发布（not prerelease）**，`releases/latest` 才会指向它；`gh release create` 默认就是正式发布。
- Release 资产名必须是 `liaisonyy.exe`、`manifest.json`、`liaisonyy-server.exe`（工作流已固定）。
- 如果仓库改名或迁移，需要同步修改客户端/服务端里的 `ycyz2505/liaisonyy`。
- 加速服务是第三方，可能随时失效；客户端会自动跳过失败项。若教室电脑完全无法访问 GitHub 和所有加速源，客户端会停在「检查更新失败」界面（强制检查的预期行为）。
