# 用 Cloudflare Pages 做客户端自动更新

> **已废弃（2026-10-19）**：0.3.2 起自动更新已改为 **GitHub Releases**（客户端 + 服务端），不再需要 Ed25519 签名密钥，也不再从 CF Pages 下载。本页仅作历史记录，请勿按此配置。当前方案见 [GitHub-Releases自动更新指南](GitHub-Releases自动更新指南.md)。

liaisonyy 的自动更新由两部分组成：

1. **清单（manifest）**：客户端从当前服务端 `GET /api/v1/update/manifest` 获取；里面有版本号、最低版本、Ed25519 签名和更新包地址。
2. **更新包**：可以是本机服务端（`/api/v1/update/package`），也可以是 Cloudflare Pages 上的绝对 URL。

Cloudflare Pages 在这里承担「更新包 CDN/镜像」的角色；清单仍由服务端提供，这样客户端不需要额外配置清单地址，也能继续使用局域网分发。如果服务端完全离线，自动更新无法检查，只能用离线包手动替换。

---

## 1. 生成签名密钥（只做一次）

在仓库根目录：

```bash
cargo run -p update-sign -- keygen
```

会输出：

```
public=<base64 公钥>
private=<base64 私钥>
```

- `public`：构建客户端和服务端时使用（环境变量 `LIAISONYY_UPDATE_PUBKEY`）。
- `private`：**离线保存**，只用来签名，绝不能提交到 GitHub、服务器或 CF Pages。

在 GitHub 仓库 Secrets 里新增 `LIAISONYY_UPDATE_PUBKEY`（值填 public）；服务端启动环境也设置同一个变量，否则服务端拒绝上传更新包。

---

## 2. 构建客户端

GitHub Actions 的 `client` job 已读取 `secrets.LIAISONYY_UPDATE_PUBKEY`。打 tag 或手动运行 workflow 后，下载 `liaisonyy-client-windows` artifact，得到：

- `liaisonyy.exe`
- `WebView2Loader.dll`

本地有 MSVC 时也可以直接构建，但必须设置同一个 `LIAISONYY_UPDATE_PUBKEY`。

---

## 3. 计算 SHA-256 并签名

```bash
cargo run -p update-sign -- sha256 liaisonyy.exe
cargo run -p update-sign -- sign 0.3.1 0.3.0 <sha256> <private>
```

- 第一个版本号是新版本。
- 第二个是最低可用版本：客户端当前版本低于它时会**强制更新**。
- 签名内容固定为：`liaisonyy-update-v1|<version>|<min_client>|<sha256>`。

---

## 4. 准备 CF Pages 资源目录

新建一个空目录，例如 `update-assets/`，放入：

```
update-assets/
├─ liaisonyy.exe
└─ manifest.json
```

`manifest.json` 示例（把尖括号替换成实际值）：

```json
{
  "schema": 1,
  "version": "0.3.1",
  "min_client": "0.3.0",
  "signature": "<sign 命令输出的签名>",
  "files": {
    "windows": {
      "url": "https://<你的项目>.pages.dev/liaisonyy.exe",
      "sha256": "<sha256 命令输出的值>",
      "size": 12345678
    }
  }
}
```

`size` 填 `liaisonyy.exe` 的字节数（PowerShell：`(Get-Item liaisonyy.exe).Length`）。

---

## 5. 部署到 Cloudflare Pages

### 方式 A：网页上传（最简单）

1. 登录 Cloudflare Dashboard → **Workers & Pages** → **Create** → **Pages** → **Upload assets**。
2. 项目名例如 `liaisonyy-update`，上传整个 `update-assets/` 目录。
3. 部署完成后得到 `https://liaisonyy-update.pages.dev`。
4. 确认 `https://liaisonyy-update.pages.dev/liaisonyy.exe` 能直接下载，且 `manifest.json` 能访问。

### 方式 B：Wrangler 命令行

```bash
npx wrangler pages deploy update-assets --project-name liaisonyy-update
```

> CF Pages 单文件上限 25 MiB；当前客户端约 6–14 MB，足够。更新包建议走 HTTPS。

---

## 6. 让服务端提供这份清单

客户端只从服务端读清单，所以需要把 `manifest.json` 放到服务端：

- 手动：保存为 `D:\liaisonyy\data\update\manifest.json`（目录不存在就新建）。
- 或管理后台「发布客户端更新」：填写版本、最低版本、签名，选择 exe，并在「更新包地址」填 CF Pages 的绝对 URL。服务端会保存清单并把 `url` 指向 CF Pages。

服务端接口：

- `GET /api/v1/update/manifest` → 返回上面的清单。
- 客户端校验签名和 SHA-256 后，从清单里的 `url` 下载 exe。

---

## 7. 验证与强制更新

- 启动旧版客户端，应弹出「发现新版本」。
- 点击「下载并更新」后，客户端从 CF Pages 下载、校验 SHA-256、替换 exe 并重启。
- 把 `min_client` 设为新版本（例如 `0.3.1`）即可强制所有旧客户端更新。
- 如果客户端提示「更新签名校验失败」：
  - 确认构建客户端时和服务端使用的是同一个 `LIAISONYY_UPDATE_PUBKEY`；
  - 确认签名内容里的版本、最低版本、sha256 与实际清单完全一致。

---

## 8. 注意事项

- **私钥不要上传**到 CF Pages、GitHub 或服务端；泄露后立即换密钥并重新发布客户端。
- CF Pages 只托管更新包；清单仍由服务端提供。服务端离线时无法检查更新。
- 客户端使用 Rust 侧 `ureq` 下载，不受浏览器 CORS 限制。
- 如果 CF Pages 自定义域名/加速，确保 URL 是 HTTPS 且长期稳定。
- 保留上一版本 exe 作为离线包，网络异常时可手动替换。
