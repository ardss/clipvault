# 多机部署状态（2026-10-12 实测）

权威源码：`K:\Project\1-Active-Projects\clipvault`（main）。发布物以 GitHub Releases 的 CI 构建为准，安装验收看 exe LastWriteTime，不看注册表版本号。

## A = DESKTOP-CD2C03F（主力机，192.168.31.64）【真实安装并运行】

- SSH：key 认证可用（用户 13397，Windows OpenSSH，默认 shell 为 cmd）
- 2026-10-12 升级迁移：旧 0.1.0（Temp 内 opencode 构建进程 + `F:\ClipVault` 旧安装）→ 官方 CI v0.1.2
- 安装路径：`C:\Users\13397\AppData\Local\ClipVault\app.exe`（currentUser 默认）
- 数据：`%APPDATA%\com.clipvault.app\clips.db` 6.9MB **真实历史，已原样保留**
- 备份：`%APPDATA%\clipvault-backup-20261012\`（db+wal+shm+settings+log，可回滚）
- 用户配置保留：热键 **Ctrl+Alt+V**、autostart=true、面板高 582
- 验证：安装器 POSTINSTALL 自动拉起（SSH 远程装也生效）；cv.log `hotkey registered: Ctrl+Alt+V` + listener + 心跳
- 遗留（保留未动）：`F:\ClipVault` 旧 0.1.0 及其卸载入口（GUID `56b0c7f1-…`）作为回滚用；桌面 `ClipVault.lnk` 已指向新 exe

## B = Du（共享 K 盘机，本会话所在机器）【操作验收通过】

- 本机开发/构建机，v0.1.2 运行中（进程自 10-07 起）
- 安装路径：`F:\Apps\ClipVault\app.exe`，热键 **Alt+V**，autostart=true（HKCU Run 确认）
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
