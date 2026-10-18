# Third-party notices: Chinese input method

liaisonyy 内置的中文输入法由以下开源项目提供：

## my_rime

- 名称：My RIME（梧桐输入法）
- 版本：0.10.9（npm `@libreservice/my-rime@0.10.9`）
- 上游：https://github.com/LibreService/my_rime
- 许可证：AGPL-3.0-or-later（见 `LICENSE-AGPL-3.0.txt`）
- 本目录文件：`rime.js`、`rime.wasm`、`rime.data`、`worker.js`
- 修改：`worker.js` 中的资源地址从 jsDelivr CDN 改为本目录（`/myrime/`），以便离线使用；引擎与词库内容未修改。
- 对应源码：上游仓库；本仓库的集成代码位于 `client/src/ime/` 与 `client/src/components/ImePanel.vue`。

## RIME 输入方案

- `@rime-contrib/luna-pinyin@0.1.1`（朙月拼音）
- `@rime-contrib/stroke@0.1.3`（笔画反查）
- 上游：https://github.com/rime/rime-luna-pinyin 、https://github.com/rime/rime-stroke
- 许可证：LGPL-3.0-only（见 `LICENSE-LGPL-3.0.txt`；LGPL-3.0 引用 GPL-3.0，见 `LICENSE-GPL-3.0.txt`）
- 本目录 `schemas/` 下的 `.bin` 与 `.yaml` 文件来自上述 npm 包，未修改。

## 合规说明

liaisonyy 客户端分发包含上述 AGPL/LGPL 组件时，需遵守相应许可证：保留版权与许可证声明、提供对应源码（本仓库与上游仓库均已公开）。如果修改了 my_rime 或 RIME 方案文件，需按 AGPL/LGPL 的要求公开修改后的源码。
