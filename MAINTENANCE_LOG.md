# ClipVault 维护日志（MAINTENANCE LOG）

> 每日维护轮（任务卡 cron `40 6 * * *`，Asia/Shanghai）在顶部追加当日记录。
> 风险登记（R-xx）是活文档：状态变更必须附证据；未实施的风险不得标记为已修复。
> 裁决来源：ChatGPT「ClipVault发版审查」固定会话（AI互通项目）。变更未经 GPT 批准不得动代码。

## 2026-10-26 维护轮（触发 06:40 +08:00；GPT 三轮裁决：M1 APPROVE → 运行时回归抓出死代码 P1 → B 路线改道 v0.1.5）

### 巡检
- 起点：`main == origin/main @ 6eee430` 工作区净；tag v0.1.4 → f26027e，Release CI #52 SUCCESS；Issues 13 open 全为 dependabot；npm 6 项 semver 内可升（只记录不升）；cargo-outdated 未装沿用 10-25 记录；官网正常（版本行 v0.1.4 同步，Latest 仍 v0.1.3）。
- 文档缺口：Draft 正文与官网 v0.1.4 变更日志均为审计轮前旧底稿，未含审计修复批次。
- 环境事实：Lead 会话无 BrowserOS MCP，浏览器/GPT 通道走任务板执行卡（agent-browser CLI 直连用户 BrowserOS CDP 9100，全卡贯通）。

### GPT 裁决（三轮，全程零方案往返）
1. 【每日维护提案，07:17】M1 APPROVE（Draft 正文补审计批次/新包 §4 重验/PNG 真实图片+旧库升级回归/证据归档）；M2 有条件 APPROVE（官网候选标注/清单 Windows B 环境限制表述/CHANGELOG 待发布日期）；M3 REQUEST_CHANGES（**SendKeys 无人值守操作真实桌面不批准**，TESTING.md:29-31 禁令无例外，UIAutomation 只读可辅助；替代=用户手动三场景或隔离 VM）；公开发布 HOLD。
2. 【M1 执行回执+新缺陷，07:47】GPT 独立核验确认 **formats.rs:113 `"PNG".encode_utf16()` 缺 NUL 终止符 → RegisterClipboardFormatW 越界读注册垃圾 ID → PNG 捕获分支真机死代码**（发布门禁级 P1；54 单测绿因未覆盖 Win32 查名；主流应用症状被 DIB 兜底掩盖；PNG-only 写入仍丢图）。修复 APPROVE（1 行 + 必补 Windows API 回归测试）；版本路线选 B：不动 v0.1.4 tag、改道 v0.1.5、旧 Draft 标 Superseded；授权「DSH 直接执行，下轮回执只需修复 diff/API 测试/PNG-only 真机回归/CI/新资产证据」。

### M1 运行时验收执行（Lead 本地段 07:25-07:33，触发时机器空闲 6.25h）
- §4 五步 PASS：全量冷备份（backup-pre-s4-20261026-072632\，db/WAL/SHM/settings/188 图 25.3MB）→ 杀 21164+6 子进程 → Unblock-File+/S → exe mtime 10-24 15:40→10-26 01:21:24（=CI 构建时刻）→ POSTINSTALL 自动拉起；settings.json 哈希前后一致、792 行/pinned 1/图 95 零损失。
- content_hash 迁移 PASS：回填 681/792 = text 324+html 357 全覆盖（image 95+file 16 本就不哈希，精确吻合）；去重 PASS（复写仍 1 行、use_count=2、时间戳挪新）。
- PNG 回归（真实剪贴板 API 写入，零合成键鼠）：PNG+DIB 双写捕获 PASS 但 cv.log 证实走 DIB 分支；**PNG-only 静默丢弃** → 抓出上述死代码缺陷。证据：clipvault-rounds\2026-10-26-s4-result.md。

### M2 文档对齐（commit 20ed0bc；main CI #90 + Pages #70 绿，GPT 独立确认）
- 官网 v0.1.4 标注「候选版本 · 验收中」+ 审计批次 ch45-47 中英注册；RELEASE_CHECKLIST §2 标注 npx tauri build 仅 CI（Windows B 环境限制，非 Tauri 通用约束）；CHANGELOG [0.1.4] 日期改 pending release。

### B 路线：v0.1.5 候选（GPT 预授权直接执行）
- 修复 6d1d493：`"PNG\0"` 终止符 + 抽出 `png_format_id()` + 2 个 Windows 级回归测试（独立注册 ID 一致性 + 真机剪贴板字节级往返）；本地 fmt/clippy/test 绿（56 passed，+2 新测试）；main CI #91 绿（windows runner 真实执行新测试）。
- 版本六处同步 0.1.5（package.json / tauri.conf.json / Cargo.toml / Cargo.lock / CHANGELOG [0.1.4] pending 折叠进 [0.1.5] 并记录 0.1.4 因缺陷未发布 / App.vue About / docs/index.html ver 行+ver_cand 候选标注+ch48）→ 584c3f5 → main CI 绿 → tag v0.1.5 推送 → **Release CI #53 SUCCESS（12 分钟）**。
- 新 Draft v0.1.5（untagged-1bc29323632f83bc4b5a，绑定 584c3f5）三资产核验 PASS，setup SHA256 三方一致 `cea5061d…e564`（2,348,268B；Card D 下载 + Lead 独立复核）。body 空（CI 新建），发布前需按 0.1.4 底稿样式回填。
- 原 v0.1.4 Draft：title/body 已加 `SUPERSEDED — DO NOT PUBLISH` 标注（Card D 落库回读 4535 字符逐字核验）。

### ⚠ 事故记录：v0.1.4 被意外发布（08:57:19），回滚进行中
- Card D 对 release 编辑表单 `form.requestSubmit()` 程序化提交，绕过前端 draft 意图逻辑 → v0.1.4（release id 422588922）翻成 published。**缓解**：title/body 已带 SUPERSEDED — DO NOT PUBLISH 醒目标注；公开 Latest 未被抢占仍 v0.1.3（API 实证）；tag/资产未动。
- 根因教训（入手册规范）：**GitHub release 表单绝不能程序化提交；draft 状态变更必须走认证 API**。
- 回滚唯一路径 = `PATCH /repos/ardss/clipvault/releases/422588922 -f draft=true`（Web UI 无转回草稿入口，gh 未登录）→ Card D 已问询用户，用户答复「你去跑，我登陆」→ 等登录完成即执行并 API 复核（本节回执后补）。

### v0.1.5 本地验收（PASS，10:34 空闲守望者自动执行）
- 触发前空闲 645s（铁律 >=600s）；冷备份 backup-pre-v015-20261026-103408\；§4 五步 PASS：exe mtime → 2026-10-26 08:59:02（=Release #53 构建时刻）ver 0.1.5，POSTINSTALL 自动拉起，settings.json 哈希不变。
- 三模式 PNG 回归（修正测试脚本 $G/$g 变量大小写冲突后 10:36 重跑）全 PASS：**PngOnly 96px** `png format read: 359 bytes` → 捕获+去重命中既有同图（use_count 1→2，缺陷前此场景静默丢弃）；**Dual 120px** PNG 路径优先（`png format read: 423 bytes`）→ 新行 816；**Corrupt 64px** PNG 读取 304 bytes → 解码失败 → 安全回退 DIB → 新行 817。附证：0.1.5 启动首事件即走新分支（517857 bytes 大图）；siw hidden + hotkey registered 在岗。证据：clipvault-rounds\2026-10-26-v05-verification.md（含首轮脚本 bug 说明：PowerShell 变量不分大小写，param 与函数体变量不得同名）。

### 发版
- v0.1.4：不发布（Superseded；误发布事故回滚中）。
- v0.1.5：HOLD——机器侧证据齐后，仍差三场景真实粘贴回归（用户手动或隔离 VM）；发布需 GPT 最终 GO/NO-GO。公开 Latest 保持 v0.1.3。

### 待办
- 用户：gh auth login（回滚解锁中）→ 三场景真实粘贴回归（拾事 tid_840001pfGDxI 已更新）。
- 机器侧：watcher 空闲触发后核对证据 → Card E 最终回执（diff/API 测试/PNG-only 真机回归/CI/新资产五样）→ v0.1.5 Draft body 回填（发布前）。
- 下轮：release 表单操作规范（API 优先/禁程序化提交）写进发版手册；隔离 VM 回归方案提案；依赖 chore 窗口（dependabot 13 PR 维持不合并，majors 需专评）。
- 经验沉淀：PCWSTR 必须 NUL 终止（读写两侧都要测）；卡自称状态须权威源核验再+1。

## 2026-10-26 凌晨 · 外部审计响应轮（用户直投审计清单："核实，有问题都修复"）

### 审计核实（13 项全部属实）
P1 图片静默丢失（`save_image` 把注册 PNG 格式字节喂给 DIB 解码器，签名被当几何数→必 None）；P2-1 siw 事件窗口外泄；P2-2 content 全文索引双倍存储；P2-3 心跳 5s 一行日志 + 256KB 整删；P2-4 图片无字节配额；P2-5 DIB 分配炸弹（20000²×4 上限内仍可 1.6GB 分配）；P2-6 zoom 常驻 ~50MB / TrySuspend 未实现；P3 捕获同步+120ms 阻塞监听线程、低级钩子装完即弃无自愈、单删/清理无 secure_delete、reconcile 持锁全量 stat、搜索仅 300 字符预览（FTS5）。**好设计清单（事件驱动/锁外捕获/self-write 序号/密码排除/粘贴身份验证）逐项确认未破坏。**

### 修复（commit 1df33dd→amend e3f97da，9 files +676）
- **P1**：`Incoming::ImagePng/ImageDib` 分流，新增 `png_bytes_to_png`（限宽高 20000 防解压炸弹）；PNG 解码失败回退读同写 DIB；回归测试钉死「PNG 字节过 PNG 路径、DIB 路径拒收」
- **P2-5**：`declared_rgba > dib.len()*2` 拒收（20MB 载荷声明 1.6GB 被断），测试覆盖
- **P2-1**：`hide_single_instance_window()`（setup 首步 FindWindowW+SW_HIDE；类名投递不受影响）
- **P2-2**：`content_hash` 列（FNV-1a 稳定哈希）+ 迁移回填 + 旧全文索引 DROP；upsert_text/html 改 hash 命中后精确比对
- **P2-3**：heartbeat 去日志；cv.log 超限滚动 cv.log.1（保留一代取证）
- **P2-4**：图片总配额 512MB（最旧未钉淘汰；stat 锁外/删行锁内/删文件锁外）
- **P3**：单删与 enforce_limit 启用 secure_delete；reconcile 两阶段（锁内取引用集合，文件系统扫描锁外）；捕获改 worker 线程 latest-wins（监听泵不再被 120ms+解码阻塞）；鼠标钩子面板可见时装/隐藏卸（专用钩子线程 PeekMessage 泵，自愈）
- **前端**：zoom 窗口 10 分钟空闲销毁（ensureZoomWin 幂等重建）
- **风险池新增**：R-FTS5 全文搜索；R-TrySuspend（webview2-com-sys 的 TrySuspend 是异步 COM handler 版，需回调封装，收益 30-60MB）

### 门禁与发布状态
- 本地：cargo fmt=0 / clippy --all-targets -D warnings=0 / **54 tests 全绿**（新增 png/dib 分流、炸弹防护、日志滚动、reconcile 两阶段、hash 迁移生产同构夹具）；npm build 绿
- main CI #87（e3f97da）SUCCESS；tag v0.1.4 重定位 e3f97da → Release #51 SUCCESS → 空提交 f26027e 重触发（badge.svg 报 failing 为 CDN 旧缓存误导，API 实证）→ Release #52（f26027e）SUCCESS
- **Draft v0.1.4 资产待登录态核验**（本轮 BrowserOS 不可用；两个 SUCCESS run 意味 action-gh-release 上传成功，匿名 expanded_assets 对 draft 仅显示源码包占位为权限边界非资产丢失）
- 发布门禁不变：GPT 裁决 + 三场景真实粘贴回归证据，之后走 `releases/edit/untagged-…` 绑定表单 draft=false

### 待办
- 登录态核验 Draft 三资产 + SHA256；GPT 提案（审计修复批次 + 发版）；三场景回归；最终验收回执

## 2026-10-25 维护轮（触发 06:40 +08:00，GPT 裁决：B 方案）


### 巡检
- 起点：`main == origin/main @ e2795d6`，工作区净（tmp-ux/ 为 DEPLOYMENT.md 引用证据，保留）；tag v0.1.4 → 1222a0b 本地核实无误。
- Issues 0 开放（13 个 dependabot PR 不变）；匿名 API 限额 0/60 烧满，巡检切登录网页通道（2FA 生效）。
- 依赖（只记录不升）：npm 6 项 semver 内（@tauri-apps/api+cli 2.12.4 / vite 8.3.5 / vue 3.5.44 / plugin-vue 6.0.10 / playwright-core 1.64.1）；cargo 传递 124 包可更（brotli 9 / cargo_toml 1.0 / dirs 7 等 major 全为传递依赖，直接依赖未动）。
- 官网正常：版本行 v0.1.4 与仓库同步（Pages 上轮绿）；changelog 领先公开 release（v0.1.3 仍 Latest），发布后对齐。
- Draft（untagged-fa08826a95d4202d6119）三资产齐、tag 绑定正确、label=Latest；**新发现 release notes 正文为空**。

### GPT 裁决（【每日维护提案 2026-10-25】，思考 42s）
**B 方案通过 · 暂缓公开发布**。APPROVE：Draft 正文回填 / 下载+SHA256 核验 / 0.1.2→0.1.4 覆盖升级核验 / 证据归档 / R-03（物理验收成功后关闭）。REQUEST_CHANGES：以 CI+单测+安装核验替代真实粘贴回归。同时钉死：覆盖升级必须保留用户数据（不清剪贴板历史/应用数据库）；Draft 正文须含「浏览器多标签页输入位置无法仅靠 HWND 完全区分」风险说明；三场景真实回归无需等 windows-mcp、人工同样有效，须记录软件/操作/结果/PASS-FAIL/日志截图；证据齐全后交最终验收回执，届时只审运行时与发布证据，不重审已通过的五轮 F1 代码。

### 执行（全部为批准项，零代码变更）
- ✅ Draft 正文回填并落库：CHANGELOG 0.1.4 底稿 + Known limitations 两节（含浏览器多标签 HWND 边界说明）；编辑页刷新回读 2009 字符完整（服务端持久化确认）。
- ✅ Draft 安装包下载（登录会话 + CDP 定向下载绕开后台节流）+ SHA256 终核 PASS：`0200ccc2…` 与 SHA256SUMS.txt 逐字一致。
- ✅ §4 覆盖升级物理核验（0.1.2 → 0.1.4，五步全 PASS）：①杀旧进程（pid 25204 及 7 个 WebView2 子进程，tasklist 确认退出）→ ②`/S` 静默安装（**新教训：浏览器下载的安装包带 Mark-of-the-Web，非交互会话下 SmartScreen 令静默安装无限挂起且无进程残留；`Unblock-File` 解除后约 30 秒完成**）→ ③app.exe 物理替换（旧值 10/1 08:38 → CI 构建时间戳 10/24 15:40，FileVersion 0.1.2→0.1.4，uninstall.exe mtime = 实际安装时刻）→ ④POSTINSTALL 钩子自动启动 → ⑤StartTime 核对 PASS。
- ✅ 用户数据保留（GPT 红线）：clips.db / clips.db-wal / clips.db-shm / settings.json 四文件升级前后 SHA256 逐字未变；启动后库行数 750 = 升级前基线 750，历史完好；捕获管线运行正常（heartbeat + 库内最新时间戳推进）。
- ✅ R-03 关闭（物理验收成功，本节即实测记录）。
- ⏸ 三场景真实粘贴回归：windows-mcp 未接入；替代通道已穷尽（技能库检索仅 9 安装玩具级不达标、DSH 插件市场无桌面控制类目）→ 待人工或 windows-mcp 接入（已登记用户待办）。
- 发版：**否**（Draft 保持候选态，公开发布等三场景回归证据）。

### 待办
- 唯一剩余 = 三场景真实粘贴回归（记事本 / 浏览器输入框 / 原生多控件）→ 证据齐 → 最终验收回执 → GPT 放行后走 `/releases/edit/untagged-…` 绑定表单发布（draft=false）。
- 依赖升级：留独立 chore 窗口（下轮提案，纯 chore 不发版）。

## 2026-10-24 15:10 收尾轮（第五轮复核 + v0.1.4 候选发布，GPT 授权）
- 15:13 GPT 第五轮增量复核（2m51s，独立读取 Rust CI 日志核验 51 passed/4 ignored，非转抄日志）：**APPROVE，放行 v0.1.4 候选 tag / Draft**；同时钉死边界——注入器最后一道焦点匹配必须保留、不得宣称消除全部竞态、**公开发布仍未授权**。
- 版本六处同步 0.1.4（package.json / tauri.conf.json / Cargo.toml / Cargo.lock / CHANGELOG.md / App.vue About / docs/index.html 含 ch41-ch44 中英双语注册）→ `release: v0.1.4`（1222a0b）→ 本地三门禁绿（51 passed）→ main CI #82 绿（1m36s）+ Pages #63 绿。
- 15:29 tag `v0.1.4` 已推 → Release CI #50 → **16:05 复核：completed successfully（26 分钟）**。
- **Draft 实核通过（16:15）**：draft 形态 `untagged-fa08826a95d4202d6119`（经典坑，编辑入口 /releases/edit/untagged-fa08826a95d4202d6119）；资产三件逐字核对 = `ClipVault_0.1.4_x64-setup.exe` + `ClipVault_0.1.4-portable.exe` + `SHA256SUMS.txt`（另有源码包）；Tag v0.1.4 绑定 1222a0b；无 Latest 徽章（v0.1.3 仍为 Latest，符合候选状态）。
- 用户验收清单（GPT 指定，全部通过才可公开发布）：①记事本真实粘贴回归；②浏览器输入框真实粘贴回归；③一种原生多控件应用回归；④RELEASE_CHECKLIST §4 覆盖升级物理核验（旧进程退出/文件替换/新进程启动）；⑤三资产核对（✅ 已由本会话完成）。未通过→继续修，不得先发布后补验收。
- 匿名 GitHub API 限额打满（60/60，15:45 重置），CI 观测切换到 github.com Actions 网页（未被 2FA 墙拦截）。
- 发版：候选阶段（Draft 已就绪）；公开发布待用户回归证据，届时走编辑页发布（draft=false 绑定表单，不用 /edit/<tag> 的 create 模式）。

## 2026-10-24 13:30 重试轮（F1 投递重试 → 误发旧版已更正，零代码）
- 13:33 副本标签 271 水合成功；13:50 误将基于第二轮契约的旧版修订案发出（分块写入途中标签假死致状态错乱）；13:55 已向 GPT 发更正——作废旧版，指回午间实施回执（ded749c）作第四轮复核基准。
- 14:00 GPT 第四轮复核（1m2s）：P1-1/P1-2/P1-3 正式 PASS 不再重开；唯一剩余 P1 = 恢复后状态可能失效（`commands.rs` Restored 臂未用 live attribution 复验，违反「恢复期状态变化立即中止」契约）。给明最小修正 + 2 个测试，授权直接修、无需新方案；tag 仍不授权。
- 14:01 并行会话按裁决提交增量 `91e1f9d`（Restored 臂 live attribution 复验 + 写握手前重验 HWND/owner-PID（target_lost_before_handshake token）+ 3 个契约测试）；本轮遵守「不抢活跃区」未碰代码，只核验 diff 与 CI（5/5 绿，含 rust 全量）。
- 14:15 增量回执已投 GPT（作者会话未发，本会话持活通道代发），待第五轮增量复核 → 通过后才进 Draft、用户真实回归与 §4 物理核验。
- 本轮零代码变更（本会话侧）；用户禁词「遗留」已从本文件清除（源头治理，两处节头 + 一处行文）。
- 发版：否。

## 2026-10-24 午间轮（触发 12:43 +08:00，F1 补投 + 实施）

### 巡检
- 起点：`main == origin/main @ 50c6170`；Issues 0（13 个 open 全为 dependabot PR，与晨间一致）；Release v0.1.3 资产齐全非 draft；npm 6 项可升（同晨间记录，未动）；官网正常（Pages 随本轮部署绿）。

### GPT 裁决（F1 修订案 · 第三轮）
**REQUEST_CHANGES · 3 项边界修正 + 授权实施**：P1-1 失效/失败不得等于安全回退；P1-2 五态枚举 + SetFocus 后验实际焦点 + child==target 边界；P1-3 hwndFocus 为身份、hwndCaret 仅辅助。其余全部 APPROVE（含 8 场景矩阵 + 兄弟控件/child==target 补充、CI→Draft→人工验收顺序、v0.1.4 patch）。明确授权：实施+验证；**不授权 tag 与公开发布**；无需第四版文字方案。

### 执行（F1，commit ded749c）
- `capture_focus`：hwndFocus 身份化，hwndCaret 仅在焦点未知/表单自焦点时辅助，入库过归属门（P1-3）。
- `restore_focus_verified` 五态枚举：SetFocus 前身份检查（alive/同线程/IsChild/自焦点），SetFocus 后 GetGUIThreadInfo 验实际焦点（P1-1/2）。
- paste 路径：child 一次性消费前置——**v0.1.3 缺陷确认**：恢复后读被 swap(0) 清零的槽位，握手恒 0、子控件匹配从未生效；前台重试成功路径此前跳过焦点恢复，已统一走验证。
- 握手决策纯函数 `handshake_child`：Restored→(target,child) 严格；NoCapturedChild→(target,0)；ChildDestroyed 仅归属验证后降级；IdentityMismatch/RestoreFailed 取消。
- cvlog 四类统计 token（focus_moved/child_destroyed/child_identity_mismatch/restore_failed），只记句柄不记标题/内容。
- 测试 +12（window 5 / injector 3 / commands 4），含真实隐藏 Win32 父子窗口身份取证；cargo test **49/49 绿**（3.3s）、fmt / clippy -D warnings 绿。
- push main → **CI 5/5 绿**（run 40065856496）+ Pages 绿。
- 实施回执（diff 摘要/矩阵/CI URL/边界声明）已投 GPT；截至收尾 GPT 未复核（第四轮），不阻塞——tag 本就未获授权。
- **GPT 第四轮复核（午间自动送达）**：P1-1/P1-2/P1-3 全部 PASS，CI #75/#76 被其独立核实；剩唯一 P1——`handshake_child` 的 Restored 分支未消费归属结果，恢复后焦点再变仍会放行。已按其最小修正落地 commit `91e1f9d`：① Restored 臂改为 `(focus_in_target == Some(child)).then_some(child)`；② 写握手前重验 (HWND, owner pid)，token=`target_lost_before_handshake`；③ Restored 失配独立 token=`focus_moved_after_restore`；④ 回归测试 3 个（attribution=None 取消 / 焦点移至兄弟控件取消 / 自焦点严格路径）。cargo test **51/51 绿**（1.8s）、fmt/clippy 绿、**CI 5/5 绿**（run 40068544841）。增量回执已投（14:15 一条为用户侧代发、内容准确；本会话误发的旧版修订案已由用户更正作废，GPT 确认忽略）。

### 发版
否（第四轮裁决明示：修复通过才放行 v0.1.4 候选 tag/Draft；第五轮增量复核待 GPT 给出）。

### 待办（触发条件见各项）
- 链路：GPT 第五轮增量复核 →（放行后）v0.1.4 候选 tag → Draft → 用户真实应用粘贴回归（记事本/浏览器输入框/一种原生多控件）+ §4 物理核验 → 才可公开发布。
- GitHub 2FA：用户侧待办（巡检仍走匿名 API）。
- 依赖升级：留功能窗口。
- 浏览器死状态教训：ChatGPT 后台标签 evaluate 反复超时，本次靠「唤醒重试 + 副本标签 271 补投 + reload 后草稿恢复」走通；重载恢复的陈旧草稿已清理（防误发）。

## 2026-10-24 维护轮（触发 06:40 +08:00）

### 巡检
- 起点：`main == origin/main @ 096e900`（昨日 site i18n 修复），CI #72 / Pages #53 绿；v0.1.3 Release 资产齐全、Latest 正常。
- Issues：开放 issue = 0（经 GitHub 匿名 API 核实）。
- 依赖（只记录不升级）：npm 6 项可升（@tauri-apps/api、@tauri-apps/cli → 2.12.4；vite → 8.3.5；vue → 3.5.44；@vitejs/plugin-vue → 6.0.10；playwright-core → 1.64.1，均在 semver 范围内）；cargo 传递依赖大量可更（含 brotli 9、dirs 7、cargo_toml 1.0 等 major，均为传递依赖；直接依赖未动）。
- 官网 ardss.github.io/clipvault：正常，changelog 最新 v0.1.3 与仓库一致，下载链指向 releases。
- 环境注意：github.com 网页被账户 2FA 登记墙拦截（GitHub 给出的剩余期限 6 天），本轮巡检改走 api.github.com 匿名端点；请用户尽快开启 2FA，否则网页巡检与发布核验受阻。

### GPT 裁决（【每日维护提案 2026-10-24】）
**REQUEST_CHANGES**：维护轨 M1–M3 批准执行（不发版）；功能轨 F1 修订方案后再审；v0.1.4 暂不放行。
另：补发了昨日未发送的 v0.1.3 终版回执，GPT 确认 v0.1.3 监督闭环归档。

### 执行
- M1 ✅ 建立本文件 + 风险池四项落盘（R-01…R-04）。
- M2 ✅ docs/TESTING.md 新增「剪贴板测试污染与清理」节（R-04 随此关闭）。
- M1+M2 为同一纯文档提交；M3 本地清理：删除 `.maintenance-commit-message.txt`、`.maintenance-focus-message-20261013.txt`（已合并提交的旧草稿）；`tmp-ux/` **保留**——docs/DEPLOYMENT.md §45 引用其截图作为走查证据，属「需保留的证据」，待解除引用后再清。
- 发版：否（纯 chore，无用户可感知变更）。

### 待办（触发条件见各项）
- R-01/R-02（F1）：已按 GPT 安全契约拟出修订案（不放宽匹配、握手句柄可信化、四态分类、8 场景矩阵、Draft 前置验收顺序）；**修订案投递受阻**——ChatGPT 后台标签反复进入死状态/会话不水合，全文已存入其输入框待发，GPT 未裁决 → 今天不动 F1 代码。已挂 2026-10-24 13:30 一次性重试（会话内提醒）；若仍未送达则次日维护轮重试。实施顺序（GPT 指定）：实现+测试 → HEAD CI 绿 → v0.1.4 候选 tag 出 Draft → 用户从 Draft 安装包完成真实应用粘贴回归 + RELEASE_CHECKLIST §4 安装升级物理核验 → 才公开发布，不得发布后才首次人工验收。
- 依赖升级：留待下次功能窗口顺带。
- GitHub 2FA：用户侧待办。
- v0.1.3 安装升级物理核验：未实测（流程缺口并入 R-03）。
- 文档提交 a022c32 的 main CI：5/5 checks 全绿（build/rust/frontend/deploy/report-build-status）。

## 风险池（2026-10-24 登记）

### R-01 焦点子控件匹配重构
- 严重度：P1（粘贴正确性，不丢不串）｜首次登记：2026-10-23（v0.1.3 监督轮）｜状态：**第四轮 P1-1/2/3 判定 PASS；唯一增量 P1 已修复待第五轮复核**（ded749c + 91e1f9d，三门禁绿 + CI 5/5 绿）｜计划版本：v0.1.4
- 问题：`src-tauri/src/win/injector.rs:19-23` 要求子控件句柄完全相等——原子控件存活但焦点转移到同窗口其他控件时误取消合法粘贴；`src-tauri/src/win/window.rs:118-132` `restore_focus()` 直接 `SetFocus()`，未验证有效性/归属；`src-tauri/src/commands.rs:391-415` 调用后忽略返回值并读取已清零句柄，注入器随后按「子控件未知」放行。
- 影响范围：粘贴注入全路径。
- 验收标准（GPT 安全契约，2026-10-24）：① 顶层校验不降级（HWND + owner-PID，任何不匹配/未知/失败即中止）；② `SetFocus` 前先验证子控件有效且归属目标窗口；③ 区分四态——有效且匹配 / 有效但焦点不匹配 / 已失效或身份不符 / 状态未知，仅明确符合回退条件才退回顶层匹配；④ `restore_focus()` 返回值必须处理，禁止静默滑入宽松匹配；⑤ 回退边界明确——已失效 + 顶层校验通过 + 当前焦点归属可确认 → 按预定义回退策略；仍存活但不匹配 → 默认取消。
- 测试矩阵（8 场景）：顶层不匹配=拒绝注入；顶层句柄为零或 PID 未知=失败关闭；匹配=放行；存活但焦点转移=默认取消（除非另有经测试的产品规则）；已销毁+焦点属已验证目标=按回退策略；句柄被复用=不得对失效身份调 SetFocus；restore_focus 失败=不得静默宽松；恢复期顶层身份变化=中止。纯决策逻辑用单测覆盖；Win32 身份检查用专用测试窗口；禁止向真实桌面发送合成键鼠输入。
- 关闭证据：修订方案 GPT APPROVE → 实现 + 矩阵单测 + 三门禁（fmt/clippy/test）+ HEAD CI 绿 → v0.1.4 候选 tag 出 Draft → 用户 Draft 安装包真实粘贴回归 + §4 物理核验 → 公开发布。
- 复核：2026-10-24 午间实施完成（ded749c）+ 第四轮唯一 P1 增量修复（91e1f9d）；第五轮增量复核 APPROVE（2026-10-24 15:13）→ 候选 tag/Draft 已建（v0.1.4，untagged 形态）→ §4 覆盖升级物理核验 PASS（2026-10-25，见当日维护轮）→ **关闭链剩余 = 三场景真实粘贴回归 → 公开发布**。

### R-02 HWND 生命周期防护
- 严重度：P1｜首次登记：2026-10-23｜状态：**与 R-01 同一修复任务**｜计划版本：v0.1.4
- 问题：`FOCUS_HWND` 被消费后直接 `SetFocus`；HWND 回收复用（含同进程内 PID 相同的场景）时 `IsWindow` + PID 校验不能完全甄别。
- 表述纪律：代码注释与发布说明使用有边界安全表述——「拦截常见失效与回收场景，不宣称消除全部竞态」。
- 关闭证据：同 R-01。

### R-03 NSIS 升级物理核验流程
- 严重度：P2（流程）｜首次登记：2026-10-23｜状态：**✅ 已关闭 2026-10-25**
- 问题：docs/RELEASE_CHECKLIST.md §4 已定义静默安装物理证据流程（LastWriteTime + StartTime），但「旧版本已安装 → 覆盖升级」场景尚无逐项实测记录；v0.1.3 未执行。
- 验收标准：对候选 Draft 安装包按 §4 五步逐项留证（杀旧进程并确认退出 → `/S` 静默安装 → 核对 exe LastWriteTime 更新 → 启动 → 核对进程 StartTime）。
- 关闭证据：2026-10-25 维护轮对 v0.1.4 Draft 安装包完成 0.1.2→0.1.4 覆盖升级五步实测全 PASS（含用户数据哈希级保留验证）；另沉淀新教训：浏览器下载的安装包带 Mark-of-the-Web 会使非交互静默安装挂起，须先 `Unblock-File`。

### R-04 TESTING.md 剪贴板测试污染说明
- 严重度：P2（文档）｜首次登记：2026-10-23｜状态：**✅ 已关闭 2026-10-24**
- 问题：手动验收会覆盖系统剪贴板并可能进入剪贴板历史，原规范未提示风险与清理边界。
- 关闭证据：2026-10-24 纯文档提交为 docs/TESTING.md 新增「剪贴板测试污染与清理」节（`git log --oneline -- docs/TESTING.md` 可查）。
