# ClipVault

**Win+V 官方增强版——好看、清爽、极简的 Windows 剪贴板历史管理器。**

`Alt+V` 呼出 → 搜索 → 点击粘贴回原窗口。纯本地运行，零网络、零账号、零遥测。

## 功能

- **无限历史**：文本、图片、文件、富文本（网页/Word 格式）、链接，重启不丢
- **搜索**：关键字即时过滤（中英文均可）
- **置顶**：常用内容固定置顶，清理历史时豁免
- **分类筛选**：全部 / 文本 / 图片 / 链接 / 文件 / 置顶
- **粘贴回原窗口**：呼出面板不丢光标，点条目内容落回原处
- **悬停预览**：截断的长文本 / 图片缩略图，悬停在外侧预览窗查看完整内容（可滚动、可划选复制部分文字）
- **键盘操作**：`↑↓` 选择、`Enter` 粘贴、`Del` 删除、`Esc` 关闭
- **深浅色主题 + 中英文界面**：面板内一键切换并记住
- **统计面板**：总量 / 分类 / 近 7 天趋势 / 最常粘贴 Top 5
- **开机自启**（可选）、托盘常驻、面板高度拖拽调整并记住

## 安装

从 [Releases](../../releases) 下载安装包，或从源码构建：

```bash
npm install
npx tauri build        # 产物在 src-tauri/target/release/bundle/
```

## 构建

Tauri 2 + Vue 3 + Rust（rusqlite / 手写 Win32 层）。

> ⚠️ 必须用 `npx tauri build` 构建发布版。裸 `cargo build --release` 不会关闭
> dev 配置，窗口将从 vite 开发服务器加载界面（详见
> [docs/postmortem-2026-09-11.md](docs/postmortem-2026-09-11.md) 用 21 小时换的教训）。

开发模式：

```bash
npx vite --port 5173        # 终端 1
npx tauri dev               # 终端 2
```

测试：`cd src-tauri && cargo test`（剪贴板往返 / DIB 解码 / DROPFILES 解析）

## 技术说明

- 剪贴板写入兼容性：图片同时提供标准 CF_DIB（自底向上）与注册 "PNG" 格式，
  Chromium 内核（Chrome/VS Code/Electron 应用）与 GDI 程序均正常读取；
  DIB 解码支持 BI_BITFIELDS 通道掩码（截图工具常用）
- 粘贴按键由常驻注入器子进程发出（部分环境下进程内合成键会被系统吞掉）
- WebView IPC 心跳看门狗：显示休眠导致通信断流时 10 秒内自动恢复
- 数据全部存于 `%APPDATA%\com.clipvault.app\`（SQLite WAL + PNG 文件）

## License

[MIT](LICENSE)
