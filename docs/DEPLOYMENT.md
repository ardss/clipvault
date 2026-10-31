# 多机部署状态（2026-10-12 实测）

权威源码：`K:\Project\1-Active-Projects\clipvault`（main）。发布物以 GitHub Releases 的 CI 构建为准，安装验收看 exe LastWriteTime，不看注册表版本号。

## A = DESKTOP-CD2C03F（主力机，192.168.31.64）【真实安装并运行】

- SSH：key 认证可用（用户 13397，Windows OpenSSH，默认 shell 为 cmd）
- 2026-10-12 升级迁移：旧 0.1.0（Temp 内 opencode 构建进程 + `F:\ClipVault` 旧安装）→ 官方 CI v0.1.2；**2026-10-30 升级 v0.1.2 → 官方 CI v0.1.6**（902 条真实历史前==后保留，备份 `%APPDATA%\clipvault-backup-20261030-a`，验收证据 clipvault-rounds/deploy-a-20261030/）
- 安装路径：`C:\Users\13397\AppData\Local\ClipVault\app.exe`（currentUser 默认）
- 数据：`%APPDATA%\com.clipvault.app\clips.db` 6.9MB **真实历史，已原样保留**
- 备份：`%APPDATA%\clipvault-backup-20261012\`（db+wal+shm+settings+log，可回滚）
- 用户热键：**Alt+V**（2026-10-30 应用户要求由沿袭自旧版的 Ctrl+Alt+V 统一还原为默认 Alt+V，settings.json.bak-20261030 留有原值）、autostart=true、面板高 582
- 验证：安装器 POSTINSTALL 自动拉起（SSH 远程装也生效，v0.1.6 起自动拉起修复会将其带入交互 session）；cv.log `hotkey registered: Alt+V`（装机时曾记录 Ctrl+Alt+V，当晚热键还原后为新值）+ listener + 心跳
- 遗留（保留未动）：`F:\ClipVault` 旧 0.1.0 及其卸载入口（GUID `56b0c7f1-…`）作为回滚用；桌面 `ClipVault.lnk` 已指向新 exe

## B = Du（共享 K 盘机，本会话所在机器）【操作验收通过】

- 本机开发/构建机，**v0.1.6 运行中**（2026-10-30 由 v0.1.5 升级：877 条真实历史前==后保留，整目录备份 `clipvault-rounds/backup-20261030-b/`；v0.1.2 时期进程曾自 10-07 连续运行）
- 安装路径：`F:\Apps\ClipVault\app.exe`，热键 **Alt+V**（cv.log `hotkey registered: Alt+V`，2026-10-30 18:41 新实例），autostart=true（HKCU Run 确认）
- 安装器含 POSTINSTALL 自动拉起钩子（2026-10-01 实测 4 秒内自启）
- K: 为 B 的本地磁盘，经 SMB 共享给 A（B 侧共享 K、K$、C$、F$、ADMIN$）

## C = lubancat（鲁班猫 RK3588，Debian 11 aarch64，192.168.31.67/.68 同机双 IP）【该平台不支持（当前形态）】

- SSH：`cat@192.168.31.67` key 认证可用；Debian 11.11，aarch64，16GB 内存，/ 剩 27G
- 桌面：无活动图形会话（tty2 登录，GNOME 会话组件残留在跑，weston 二进制在但未运行）
- 依赖：libwebkit2gtk-**4.0**（Tauri 2 需要 4.1）；无 xclip/wl-paste
- 结论：当前没有桌面剪贴板会话，且剪贴板层是 Win32 专有实现——**不做全量移植，不部署查看组件凑数**。若未来 C 接显示器跑 Wayland 桌面且确有需求，再评估（需 Tauri 2 Linux 侧重写剪贴板层，属新项目立项而非维护）

## 隐私红线（长期有效）

- 剪贴板历史只存本机，跨机同步默认关闭，未经明确授权不得上传任何历史到 C 或网络
- 测试数据只用虚构内容；禁止向用户真实窗口发送合成键鼠输入
- 公开发版/tag/开端口需用户明示

## SOP 速查

- 静默安装：`installer.exe /S`（装完自动拉起）；验证看 exe LastWriteTime + 进程 StartTime + cv.log 热键行
- 杀旧进程：`Get-Process app | Stop-Process -Force`；重编译前先杀 cargo/rustc
- GitHub API：curl 必带 `--ssl-no-revoke`；发版后 draft 需手动 PATCH `{"draft":false}`
- 升级迁移：先备份 db/wal/shm/settings，停进程后二次复制 db，再装新包
- **部署后配置校验（每次部署必做，只读）**：在 B 机跑 `powershell -File scripts/check-machine-config.ps1 -Compare a-machine -SshHost a-machine`——经 SSH 单行（`powershell -NoProfile -EncodedCommand`，规避 cmd 转义）读取 A 机本地 `%APPDATA%\com.clipvault.app\settings.json` 的 hotkey/autostart/panel_height 并与 `docs/machine-registry.json` 登记值对比，对比在 B 机侧完成。**只读校验，允许经 SSH**；禁止把安装/卸载/合成输入外推到 SSH。输出 DRIFT 时：若属用户改偏好，先更新 machine-registry.json（登记滞后是流程缺陷），再核机器配置。定位是部署后检测层（事故 #7 热键继承发生在安装时点，本检查只能事后发现）

## 产品维护迭代 #1（2026-10-12）

真实 UI 走查（Vite + demo-shim 沙箱，Playwright 驱动，截图在 tmp-ux/，未触碰真实剪贴板/桌面）：

**已修复（095c955）**
1. [P1] 搜索无结果时误显示「暂无内容，复制任意内容…」的采集空态文案 → 改为「没有匹配的结果 + Esc 清空提示」
2. [P1] 置顶条目插入日期分组导致「今天/昨天」表头重复 → 置顶独立成组

**缺陷池（按优先级，下一迭代候选）**
3. [P2] 置顶分组标签「置顶」与过滤 tab 同名，视觉上易混（可改用图钉图标+浅色标签）
4. [P2] toast 提示 2.6s 后消失，粘贴失败这类需要行动的提示与普通确认无差异化
5. [P2] 列表无虚拟化（历史上限 5000 时长列表渲染压力待测；10 万行压测证明 DB 层无虞，前端未测）

**性能基线（B 机，运行 5 天）**：33MB RAM、CPU 累计 271s（可忽略）、db 3.9MB、WAL 已被检查点收干净、日志 180KB —— 容量/内存策略全部按设计工作。

**下一版本目标**：缺陷池 3-5 + 用户真实反馈；发版前需用户批准。
