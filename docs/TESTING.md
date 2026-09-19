# 测试标准（Testing Standard）

> 本文是 ClipVault 的质量/测试流程规范。发布操作流程见 [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md)。

## 1. 门禁（CI 已强制）

每次提交必须全部通过，缺一不可：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
npm run build
```

## 2. 分层测试

| 层级 | 内容 | 运行方式 |
|------|------|----------|
| 单元测试 | 各 crate 内 `#[cfg(test)]`，随门禁运行 | `cargo test` |
| 深度测试 (stress) | 标记 `#[ignore]` 的深度套件（压力/长路径） | `cargo test -- --ignored` |
| 故障注入 | 模拟异常路径（崩溃、损坏数据等） | `cargo run --example failinject` |
| Demo 页 E2E | Playwright 跑 `docs/demo`，**唯一允许的 UI 自动化** | 见 `docs/demo` 说明 |

**关键原则：每一处新的代码路径，尤其是涉及窗口生命周期、线程与 IPC 的改动，必须经过运行时验证。代码审查通过不等于测试通过。**

浏览器沙箱（Playwright on `docs/demo`）运行在独立环境中，不会触碰真实桌面，因此是安全的 UI 验证方式。

## 3. 禁止事项（硬性规则，无例外）

1. **严禁对用户真实桌面发送合成键鼠输入**（`SendKeys` / `mouse_event` / `SetCursorPos` 等）。测试脚本的按键和鼠标移动会落进用户正在使用的聊天窗口、编辑器，属于破坏性行为。任何 UI 自动化只允许跑在 Playwright + `docs/demo` 沙箱里。
2. **严禁在低级钩子（keyboard/mouse hook）回调里做文件 I/O**。钩子回调有严格时限，I/O 会卡住整个系统的输入队列。
3. **严禁在同步 tauri command 里做可能阻塞的窗口/UI 创建**。在主线程上同步等窗口创建完成会死锁主线程（曾导致 P0 事故：lazy window creation + 同步等待 = 全局假死）。窗口创建一律异步、事件驱动。
4. **安装验证必须核对物理证据**：安装后 exe 的 `LastWriteTime` 与进程的 `StartTime`，不能只看注册表里的版本号。版本号可能没变但文件没被替换，或旧进程根本没退出。

## 4. 手动验收清单（release 前人工过一遍）

- [ ] 复制文本/图片/文件后，`Alt+V` 唤出面板，条目正常显示
- [ ] 点击条目能粘贴回原应用（剪贴板回写 + 注入子进程生效）
- [ ] 悬停条目展开侧边预览，长文本可滚动、可选中
- [ ] 热键冲突场景：占用 `Alt+V` 时重注册失败有提示，换键后设置持久化
- [ ] 设置（主题/语言/面板高度/保留策略）重启后保留
- [ ] "清空历史"有两次确认，确认后 SQLite 与图片文件真的清空
- [ ] 深色/浅色主题切换正常，中/英双语切换正常
- [ ] 暂停捕获开关生效（暂停期间复制不进历史）
- [ ] 超大文本（>256 KB）走文件旁路，预览可看
- [ ] `grep`/扫描产物与日志，确认无 emoji 混入代码与 UI 文案

## 5. 日志

`cv.log` 常开（256 KB 滚动上限），位于 `%APPDATA%\com.clipvault.app\`。

**排障第一步永远是读 `cv.log`**，不要靠猜。IPC 心跳、窗口重载、钩子异常、粘贴注入等关键路径都会写日志。
