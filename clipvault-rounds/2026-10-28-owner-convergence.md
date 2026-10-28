# 2026-10-28 轮次记录：CV 开发 Owner 收敛（本会话）

角色确认：本 ZCode 会话（K:\Project\1-Active-Projects\clipvault）是 CV 唯一代码修复/QA/候选发布入口。DSH 的历史提交经核查**全部为 docs 维护轮，无代码改动**，其仓库权限已撤回。

## 继承（不重复研发）
- HEAD `9dbc614` == origin/main；候选代码与 tag v0.1.5 **逐字节一致**（git diff v0.1.5 HEAD -- src-tauri src package.json 为空）
- PNG P1 根因修复：`formats.rs` NUL 终止符（6d1d493）+ 2 个 Win32 级回归测试
- v0.1.5 draft（id 423764142）资产齐全：setup/portable/SHA256SUMS，CI 从 tag 构建
- v0.1.4 误发布：平台限制无法转回 draft，body 已加固警告，Latest 保持 v0.1.3，暴露下载 5 次（见 2026-10-27 取证）
- 沙箱未启用（需管理员授权），WSB/VM 验收包就绪未用

## 本轮新增证据（2026-10-28）
1. **门禁**：cargo test 56/56（含 png_format_id_matches_independent_registration、png_roundtrip_via_clipboard）、clippy -D warnings 零警告
2. **真实来源 PNG 捕获【PASS】**：测试 identifier 构建（com.clipvault.test 隔离 profile），PowerShell P/Invoke 以注册格式 "PNG"（fmt=49358）写入 96x40 虚构 PNG，**无 DIB、无文本伴随** → 存储行 `[Image 96x40]`、落盘 PNG 结构合法、缩略图生成。走的正是修复后的专用分支，非 DIB 回退。
3. **数据保留【PASS】**：隔离 profile 3 行跨实例重启完好；真实 db 全程未动
4. **粘贴/焦点回归【未执行——按红线终止】**：合成键鼠路线在 Alt+V 发送前前台被用户侧 LenovoAppStore 窗口抢占，DOWN+ENTER 两键落入用户窗口（面板未获前台、粘贴未发生、TextBox 未变化、无数据影响）。据此确认：合成输入路径在真实桌面不可控，永久弃用；三场景粘贴回归只能走 (a) 用户手动 5 分钟验收（clipvault-rounds/manual-acceptance-guide-2026-10-28.md 已就绪）或 (b) 用户启用沙箱后跑验收包。

## 事故与止损（如实记录）
- 隔离尝试 1（APPDATA 环境变量覆盖）无效：Tauri 数据目录走 Known Folder，测试实例短暂写到真实 profile，捕获了 1 行测试 PNG → 已备份 db 后精准删除 id=83，其余 825+ 行无损，settings/热键/自启未变
- 真实实例测试期间短暂停止，已恢复并确认 Alt+V 重新注册、心跳正常

## 发布门禁状态（v0.1.5 HOLD 依据）
| 门禁 | 状态 |
|---|---|
| 代码与 tag 一致 | ✅ |
| cargo test / clippy | ✅ 56 pass / 0 warn |
| 真实来源 PNG 捕获 | ✅ 本轮补齐 |
| 数据保留/迁移 | ✅ |
| 三场景真实粘贴 + 焦点回归 | ⏳ 待用户手动验收（指引已备好）或沙箱 |
| GPT《ClipVault发版审查》最终 GO | ⏳ 验收通过后投审 |
| 发布操作 | GO 后本会话执行 draft→published，随后通知 WinA/WinB 部署 |
