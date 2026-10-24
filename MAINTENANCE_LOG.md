# ClipVault 维护日志（MAINTENANCE LOG）

> 每日维护轮（任务卡 cron `40 6 * * *`，Asia/Shanghai）在顶部追加当日记录。
> 风险登记（R-xx）是活文档：状态变更必须附证据；未实施的风险不得标记为已修复。
> 裁决来源：ChatGPT「ClipVault发版审查」固定会话（AI互通项目）。变更未经 GPT 批准不得动代码。

## 2026-10-24 15:10 收尾轮（第五轮复核 + v0.1.4 候选发布，GPT 授权）
- 15:13 GPT 第五轮增量复核（2m51s，独立读取 Rust CI 日志核验 51 passed/4 ignored，非转抄日志）：**APPROVE，放行 v0.1.4 候选 tag / Draft**；同时钉死边界——注入器最后一道焦点匹配必须保留、不得宣称消除全部竞态、**公开发布仍未授权**。
- 版本六处同步 0.1.4（package.json / tauri.conf.json / Cargo.toml / Cargo.lock / CHANGELOG.md / App.vue About / docs/index.html 含 ch41-ch44 中英双语注册）→ `release: v0.1.4`（1222a0b）→ 本地三门禁绿（51 passed）→ main CI #82 绿（1m36s）+ Pages #63 绿。
- 15:29 tag `v0.1.4` 已推 → Release CI #50 运行中；产物 Draft 生成后进入用户验收阶段（见下）。
- 用户验收清单（GPT 指定，全部通过才可公开发布）：①记事本真实粘贴回归；②浏览器输入框真实粘贴回归；③一种原生多控件应用回归；④RELEASE_CHECKLIST §4 覆盖升级物理核验（旧进程退出/文件替换/新进程启动）；⑤安装包+便携版+SHA256 三资产核对。未通过→继续修，不得先发布后补验收。
- 匿名 GitHub API 限额打满（60/60，15:45 重置），CI 观测切换到 github.com Actions 网页（未被 2FA 墙拦截）。
- 发版：候选阶段（Draft）；公开发布待用户回归证据。

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
- 复核：2026-10-24 午间实施完成（ded749c）+ 第四轮唯一 P1 增量修复（91e1f9d）；关闭链剩余 = GPT 第五轮增量复核 →（放行）v0.1.4 候选 tag → Draft → 用户 Draft 真实回归 → §4 物理核验 → 发布。

### R-02 HWND 生命周期防护
- 严重度：P1｜首次登记：2026-10-23｜状态：**与 R-01 同一修复任务**｜计划版本：v0.1.4
- 问题：`FOCUS_HWND` 被消费后直接 `SetFocus`；HWND 回收复用（含同进程内 PID 相同的场景）时 `IsWindow` + PID 校验不能完全甄别。
- 表述纪律：代码注释与发布说明使用有边界安全表述——「拦截常见失效与回收场景，不宣称消除全部竞态」。
- 关闭证据：同 R-01。

### R-03 NSIS 升级物理核验流程
- 严重度：P2（流程）｜首次登记：2026-10-23｜状态：**开放**｜计划版本：v0.1.4 发版时执行
- 问题：docs/RELEASE_CHECKLIST.md §4 已定义静默安装物理证据流程（LastWriteTime + StartTime），但「旧版本已安装 → 覆盖升级」场景尚无逐项实测记录；v0.1.3 未执行。
- 验收标准：对候选 Draft 安装包按 §4 五步逐项留证（杀旧进程并确认退出 → `/S` 静默安装 → 核对 exe LastWriteTime 更新 → 启动 → 核对进程 StartTime）。
- 关闭证据：v0.1.4 Draft 安装包升级实测记录（本文件或 RELEASE_CHECKLIST 勾选项）。

### R-04 TESTING.md 剪贴板测试污染说明
- 严重度：P2（文档）｜首次登记：2026-10-23｜状态：**✅ 已关闭 2026-10-24**
- 问题：手动验收会覆盖系统剪贴板并可能进入剪贴板历史，原规范未提示风险与清理边界。
- 关闭证据：2026-10-24 纯文档提交为 docs/TESTING.md 新增「剪贴板测试污染与清理」节（`git log --oneline -- docs/TESTING.md` 可查）。
