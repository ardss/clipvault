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

## 增补（2026-10-28 19:28-20:08 · 额度恢复续办轮）

### 隔离方案穷尽结果
| 方案 | 结论 |
|---|---|
| Windows Sandbox | ❌ wsb.exe 不存在，未启用（启用需管理员+重启，用户排除项） |
| Hyper-V / VirtualBox / VMware | ❌ 服务未运行 / 未安装 |
| APPDATA 环境变量隔离 | ❌ Tauri 数据目录走 Known Folder（上轮事故已证） |
| **测试 identifier 构建（com.clipvault.test）** | ✅ 数据目录彻底隔离，PNG 捕获/保留测试均基于此 |
| **CDP 面板内可信点击**（WebView2 --remote-debugging-port=9223，`show_panel_cmd` 编程唤出面板，替代全局热键） | ✅ 机制可行（面板页 http://tauri.localhost/ 可达、行可点击），全程零合成键鼠 |
| 合成键鼠（SendKeys/AppActivate 抢前台） | ❌ 已弃用：活跃用户桌面下前台不可抢，4 次 fail-safe 全部正确中止（零输入落桌）；20:05 实测前台=用户 Chrome（kynoptic 页）证实用户正活跃 |

### 构建教训
裸 `cargo build --release` 不启用 custom-protocol → 面板加载 devUrl 失败（chrome-error）；必须 `npx tauri build --no-bundle`。

### 当前部署：空闲门控 watcher
- `tmp-ux/idle-watcher.ps1`（已布防，20:08 armed）：桌面空闲≥10min → 停真实实例 → 起测试实例(CDP) → 起受控靶（WinForms 双控件/记事本/独立 profile Chrome）→ 跑 driver.mjs（四场景，逐场景前台硬校验，不匹配即安全中止）→ 恢复真实实例。证据落 `C:\Users\13397\cv-iso-test\{driver.log,driver-output.txt,host-state.txt,targets.log}`。
- 唯一非合成"按键"=应用自身 injector 的 Ctrl+V，受其 fail-closed 焦点契约约束，目标是毫秒级前校验过的受控窗口。
- 定时复核：automation-c70858cb（23:08 一次性）检查结果并推进 GPT 终审；GO 前仍 HOLD 不发版。

### 23:08 定时复核结果（automation-c70858cb 执行）
- watcher 20:09 触发，但 idle 计算踩中 TickCount 49 天回绕（假大值）→ 在用户活跃时启动
- driver 4 场景前台硬校验**再次全部正确拦截**（foreground=9464 用户 Chrome ≠ 靶 pid），零输入落桌
- 真实实例由 watcher finally 块自动恢复（20:10，`hotkey registered: Alt+V` 确认）
- 修复：idle 改 uint 回绕减法（sanity 23:10 idle=479234ms 合理），门槛提到 15min；watcher 已重新布防（23:10 armed）
- 下一复核：一次性 automation（约 +3h），条件与红线不变

### 闲时复核 #2（offpeak-0677653c 执行）
- watcher 23:17 触发（idle=900s 达 15min 门槛，计算正确），全流程跑完并自愈恢复真实实例（23:18:16，Alt+V）
- driver 4 场景**第三次全部被前台硬校验拦截**：foreground=9464（BrowserOS Chrome，kynoptic 页）≠ 靶 pid。该"空闲"期间 9464 持有前台但无键鼠输入（可能后台渲染/播放态）→ GetLastInputInfo 无法覆盖此类占前台
- 零合成键鼠落桌，真实 clips.db 全程未动
- **结构性结论**：9464 是长驻前台窗口，只要它开着，AppActivate 抢不到前台、空闲门控也不可靠。机器侧自动化四场景粘贴在本机（用户活跃共桌）已穷尽：Sandbox/VM 不可用 + 前台不可控。剩余唯一路径=用户 5 分钟手动验收（manual-acceptance-guide-2026-10-28.md 四步，回 PASS/FAIL 四行）或管理员启用沙箱跑验收包
- 停止重新布防 watcher（避免反复短暂停启真实实例）；门禁表"四场景粘贴"项正式转为待用户输入

### 10-29 恢复轮：根因修复 + 前两场景 PASS（13:35 布防中）
**根因链（区别三类失败）**：
1. `ABORT foreground != target` = **测试环境失败**：Windows 前台锁定策略使 AppActivate 无法从长驻前台进程抢焦点。解法：`AttachThreadInput`+`SetForegroundWindow`（窗口管理 API，非键鼠注入）——`tmp-ux/focus-take.ps1`
2. `landed=false 但 paste sent` = **测试架构假设错误**：宿主 `while{Sleep}` 循环不抽消息 → WinForms Timer 从不 Tick → 焦点信号失效、状态文件陈旧、粘贴全落默认 TB。解法：显式 `DoEvents` 消息泵
3. 结果不可观测：targets.json BOM、host-state 半写竞态、`slice(0,2)` 把 RTB 切成 "RT"、跨轮基线污染——逐一修复
**产品结论：粘贴链路（capture→paste_clip→clipboard write→foreground restore→focus handshake→injector Ctrl+V）三轮日志全部正确**（cv.log: write_ok=true, restored=true, target/focus hwnd 与外部校验一致, sent）
**实测 PASS（13:13 轮 driver.log）**：
- textbox: landed=true, otherKept=true, **PASS=true**
- richtextbox: landed=true, otherKept=true, **PASS=true**
- notepad/chrome: 前两轮被 pid 校验/Chrome 更新弹窗挡住 → 修复（title 发现免 pid、Edge 靶）后由空闲 watcher 自动补跑

### 10-29 14:4x 状态快照
- 已 PASS（13:13 轮实证）：textbox、richtextbox（真实粘贴+焦点回归，RESULT PASS=true）
- 待自动补跑：notepad（单实例换 pid → 已改 title 子串发现）、chrome（更新弹窗 → 已换 Edge 靶 + CDP 就绪轮询）
- 修复版 watcher 已布防（14:07，CDP 就绪轮询 + 15min 空闲门控 + finally 自愈）；用户持续活跃（36min 无 15min 空闲窗），自动触发未发生
- 交接约定：watcher 触发后 driver.log 会出现 4 行 RESULT；任一会话消息即可让 Owner 解析结果并执行「组证据包→GPT 终审→GO 后发布」
