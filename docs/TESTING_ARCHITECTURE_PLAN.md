# 测试体系架构升级规划（ClipVault）

> 起草日期：2026-10-31
> 依据：五域盘点（Rust 单测/前端可测性/Win32 原生/CI 与发布/历史事故反推），证据均来自本次会话实跑命令与仓库文件（引用格式 `路径:行号`）。
> 约束：五层金字塔；每层回答一类 bug；优先拦截历史事故；不引入重型商业工具；E2E 原定"WebdriverIO + @wdio/tauri-service（webview 层）+ UIA/守卫扫描（原生层）"——评审修订（2026-10-31）裁撤 WDIO 线（v1.5.1 与 Tauri 2.11/WebView2 兼容性未验证、为 3-4 条用例引入整套 driver 工具链收益为负，且 playwright-core 降级方案 tests/maintenance-browser-e2e.mjs 已存在且工作），webview 层改用 playwright-core；性能阈值卡 P99，行为证明走真机探针（criterion 同批裁撤，理由见 §3.3）。
> 本文档只做规划，不改任何代码。

---

## 1. 现状与缺口总结

### 1.1 现状基线（本次实跑确认）

| 维度 | 现状 | 证据 |
|---|---|---|
| Rust 单测 | 88 个 `#[test]`，全为 crate 内 `#[cfg(test)]`；实跑 `cargo test` = 84 passed / 0 failed / 4 ignored | grep 计数 88；实跑输出 |
| Rust 集成测试 | **无集成测试**：`src-tauri/tests/` 目录存在但为空（2026-09-19 创建） | `ls -la src-tauri/tests` → 仅 `.`/`..`，无任何 .rs 文件 |
| IPC 层 | 16 个命令（lib.rs:358-375 注册）零真实 invoke 覆盖，tauri mock runtime 完全未用 | `grep -rn 'tauri::test|mock_app|MockBuilder'` 为空；src-tauri/Cargo.toml:19 无 test feature、无 dev-dependencies |
| 前端 | 无任何单测设施：无 vitest/@vue/test-utils，无 test 脚本，无 *.spec.*；逻辑全闭包在 App.vue 713 行单组件 setup() 内 | package.json:6-10；App.vue:23-201 |
| 前端唯一测试 | tests/maintenance-browser-e2e.mjs（node:test + playwright-core），未接入 npm scripts，无文档引用——孤岛测试 | package.json scripts；grep docs 为空 |
| 守卫测试 | 仅 1 条（hook_pump_must_not_sleep，win/mod.rs:313，源码字符串扫描） | `grep -rn 'must_not\|guard' src-tauri/src/win/*.rs` 仅 1 命中 |
| 基准 | 4 个 `#[ignore]` 基准中 **deep_10k_vs_100k_scale_bench 当前实跑是红的**（2026-10-31 本会话复跑 `cargo test --lib deep_10k -- --ignored` 复现：失败点 **db.rs:1154-1156**，`assert_eq!(victims.len(), n_text + n_img + 200 - limit)` left: 9700 / right: 9500，0.75s 即失败——**确定性计数断言失败，非时序抖动**：多出的 200 条正是应被 dedup 的 upsert 未去重，属行为回归信号；该基准另含时序性断言 `ratio < 3.0`（db.rs:1253），debug/慢机上有假红风险，见 §3.3。初版引 "db.rs:1171" 为函数起点行号偏移；"608s" 是全套 4 个基准的耗时，非本条） | 本会话实跑输出：`thread 'db::stress_tests::deep_10k_vs_100k_scale_bench' panicked at src-tauri\src\db.rs:1155:9: assertion left == right failed: left: 9700 right: 9500 ... test result: FAILED` |
| CI | ci.yml/release.yml 只跑 `cargo test`；`--ignored` 深度套件不在门禁；checklist 第 2 节（RELEASE_CHECKLIST.md:21）要求但靠人记得 | .github/workflows/ci.yml:36-37 |
| 版本一致性 | 五处版本仅 1/5（tag==tauri.conf.json）有机器校验 | release.yml:12-17 |
| 真机 e2e | scripts/e2e-paste.sh 存在但被 TESTING.md:31 明令禁止在用户桌面跑，无隔离环境落实，进不了任何门禁 | docs/TESTING.md:31、scripts/e2e-paste.sh:33 |

### 1.2 缺口汇总（按严重度）

- **High（5）**：IPC/mock runtime 零覆盖；无集成测试目录；LL 钩子延迟性质无行为级证明（守卫可被等价写法绕过）；热键/托盘零自动化；前端零单测 + paste 真机链路无合规执行环境。
- **Medium（6）**：autostart 零测试；剪贴板 listener 时序零测试；守卫模式未沉淀复用；版本一致性 1/5；安装包新鲜度未进 release.yml；打包冒烟/多机配置漂移/发布缺陷闭环守卫缺失。
- **Low（3）**：弱断言冒烟测试伪装覆盖；真窗口测试能否过 GitHub Actions windows runner 未验证（本会话未跑 CI，如实声明）；发布后 §4 可脚本化项未脚本化。

### 1.3 核心结构性矛盾（必须如实指出）

1. **守卫测试的原理性局限**：`hook_pump_must_not_sleep` 是字符串扫描，任何避开 `sleep(` 字样的等价阻塞写法（PeekMessage+WaitForSingleObject 超时）都能绕过。mod.rs:331 注释自己承认"延迟性质是 code-review invariant"。**字符串守卫只能作为底线，行为级探针（idle-gated 延迟测量）才能证明性质**——两者都要，不能只靠前者。
2. **e2e-paste 的合规悖论**：探针架构（paste-probe.ps1 的 textbox→文件镜像）是对的，但对用户真实桌面发合成输入被 TESTING.md:31 硬性禁止。解法不是改规则，而是**给这条链路一个隔离执行环境**（专用测试账户 / VM / 独立交互会话），TESTING.md:60 已写"优先在隔离环境执行"但没有落实脚本——本规划补上落实，不解除禁令。
3. **"开发机 = 生产机"的现实**：B 机（本开发机）正是 v0.1.6 生产实例运行中的用户机器（DEPLOYMENT.md:24-26，877 条真实历史）；A 机通过 SSH 访问、默认 shell 为 cmd（DEPLOYMENT.md:7）。这带来三重约束，任何探针/e2e 设计必须先回答：(a) scripts/e2e-paste.sh:9 开头就 `taskkill //IM app.exe //F`——在 B 机直接跑会**杀掉用户生产实例**；(b) app 依赖 tauri-plugin-single-instance（src-tauri/Cargo.toml:21），不先停生产实例，探针进程会被单实例机制移交/顶替，clipboard_listener_probe 的 idle-gated 设计**在生产实例共存时根本不成立**；(c) 探针写剪贴板必须按 TESTING.md §6 规则：测试前留存、测试后恢复、只清理测试条目、禁止清空整个历史。因此**所有原生探针（hook 延迟、clipboard listener、paste e2e、UIA 托盘探针）统一用 `CV_E2E_ISOLATED=1` 一道门**：缺该 env 直接拒绝执行；门内流程 = 停生产实例（**路径显式：`F:\Apps\ClipVault\app.exe`，DEPLOYMENT.md:26**）→ 留存剪贴板 → 跑探针 → 恢复剪贴板 → 重启生产实例，执行窗口须事先获**用户明示授权**（对齐 DEPLOYMENT.md 隐私红线的"用户明示"标准）并记入 MAINTENANCE_LOG。隔离形态采用**在位隔离**（同一桌面、生产实例已停、剪贴板已保护），不依赖脚本化第二交互会话——Windows 上 fast user switching 无法无人值守切入、RDP 断开后合成输入落锁屏、VM 需在生产机装虚拟化，第二会话路线**方案未解决**，降级形态见 §3.4"隔离载体"。**SSH 禁区**：探针/paste e2e **永不在 A 机经 SSH 跑**——SSH 会话非交互桌面（DEPLOYMENT.md:11 POSTINSTALL 拉起依赖已登录桌面可佐证），合成输入落不进用户可见桌面；SSH 单行模式**只用于只读检查**（配置守卫、进程确认），不得外推到任何破坏性/合成输入操作。
4. **deep_10k_vs_100k_scale_bench 已红（2026-10-31 本会话实跑复核确认）**：失败是 db.rs:1154-1156 的确定性计数断言（9700 vs 9500，应 dedup 的 200 条 upsert 未去重），不是时序问题——独立于该基准内的时序断言 `ratio < 3.0`（db.rs:1253）。两者裁决分开：计数回归在阶段 0 修根因；ratio 断言因跨机时序敏感（与 §3.3 裁撤 criterion 的理由同构）移出门禁、降为真机校准项。

---

## 2. 事故 → 缺失测试层映射表

来源：勘察员·历史事故盘点（git log --oneline -40 + CHANGELOG.md 0.1.2–0.1.7，初版 9 起，评审补入第 10 起用户数据险毁级事故）。此表是优先级排序的依据：**每层优先落地能拦截已发生事故的那部分。**

| # | 事故 | 证据 | 根因层 | 本可拦截的测试层 | 对应本次交付物 |
|---|---|---|---|---|---|
| 1 | 鼠标钩子 pump 内 40ms sleep，全局输入延迟（cf2e7db 修复） | src-tauri/src/win/mod.rs:313-334 | 原生 Win32 | ① 源码守卫（已有）② **行为级延迟探针：装 WH_MOUSE_LL，测回调间隔 P99** | L2/L3 延迟探针 `src-tauri/tests/hook_latency_probe.rs`（真机 P99，CV_E2E_ISOLATED 门） |
| 2 | NUL 字节进入剪贴板写入路径（6d1d493） | CHANGELOG | Rust 单元 | 纯函数单测（部分已有 dib/formats 测试） | L1 既有覆盖保持，补 NUL/超长 payload 参数化用例 |
| 3 | handshake 焦点恢复失败 focus=0（0.1.5） | CHANGELOG.md:40 | 原生 Win32 | 焦点矩阵已有单测（window.rs 9 个）；缺**注入器→真实落点 e2e** | L4 隔离环境 paste e2e（合规化 e2e-paste.sh） |
| 4 | NSIS 安装器硬编码 app.exe 断裂（0.1.4 邻域） | CHANGELOG.md:22 | 打包 | 安装→启动冒烟 | L5 `scripts/smoke-install.ps1` 进 release.yml |
| 5 | workspace 非 root 成员 profile 被 cargo 静默丢弃，opt-level/lto/strip 消失 | CHANGELOG.md:26 | 构建配置 | **产物守卫：断言 release 二进制确实被 strip/opt** | L5 `scripts/check-build-profile.ps1` |
| 6 | fetch-on-open 条件漏 text_path，>256KB 文本打开截断 | CHANGELOG.md:21 | 前端逻辑 | 前端单测（fetch 触发条件抽成纯函数）+ webview e2e | L1 `src/modules/openPolicy.js` + vitest；L4 playwright-core webview 用例 |
| 7 | A 机热键被继承成 Ctrl+Alt+V（9d2a1dc，docs-only 修复） | git log；DEPLOYMENT.md:19（2026-10-30 还原 Alt+V） | 多机配置漂移 | **部署后检测层（如实降级：不是拦截层）**——热键继承发生在安装/POSTINSTALL 时点，配置校验只能事后发现，不能防住安装瞬间。且形态必须是部署时经 SSH 执行的校验脚本：热键配置在各机 `%APPDATA%\com.clipvault.app\settings.json`，仓库里没有该文件，GH runner 读不到（必失败或空转），开发机读到的只是 B 机配置——而事故恰好发生在 A 机。cargo 集成测试在此**永远不会在漂移发生地运行** | L2/L5 `scripts/check-machine-config.ps1`（经 SSH 单行 `powershell -Command`，期望值由 B 机 registry 传参，进部署 SOP 而非 CI） |
| 8 | v0.1.4 带 P1 缺陷误发布 + 意外**公开**（abd36ee；CHANGELOG 0.1.5 段：2026-10-27 程序化提交被 GitHub 按发布处理） | git log；ef4fc8c；RELEASE_CHECKLIST §7（4 条硬门禁） | 发布流程 | **两个半边、两套拦截**：① 缺陷半边——机器可执行的"未闭环 P1 则 tag 拒绝"守卫（release-gate.ps1）；② 公开半边——RELEASE_CHECKLIST §7 的 4 条门禁（禁程序化提交、draft 变更只走认证 API、写后读验证、单独 GO 授权）目前是纯文档规则，机器侧只能做"发布动作后 draft→published 状态写后读验证"，程序化提交禁令靠 checklist 签核强制（如实承认：无法用测试完全拦截，只能加验证点收窄） | L5 `scripts/release-gate.ps1`（P1 闭环 + draft 状态写后读验证）+ checklist §7 保持人工签核 |
| 9 | paste 验收长期 "awaits user manual run"（546a6dd、a5ab53f） | git log | 真机 e2e 缺位 | 隔离环境端到端（同 #3） | L4 |
| 10 | **armed clear-history 事故**：设置面板关闭后未撤销已 armed 的"清空历史"确认，无关点击可wipe 全部历史 | CHANGELOG.md:73（0.1.5 段修复记录） | 前端状态机 | 确认-armed 状态机抽成纯函数后单测：任何"面板关闭/切页/失焦"事件必须先 disarm，用状态转移表穷举断言 | L1 `src/modules/confirmArmed.js`（新，阶段 3）+ vitest 转移表用例 |

**结论**：10 起事故中 5 起（#1、#3、#4、#5、#9）落在"原生/打包/真机"层——这正是当前最薄弱处；3 起（#2、#6、#10）单测层可拦（含 #10 用户数据险毁级）；2 起（#7、#8）是流程/配置守卫。规划顺序据此排定。

**自列缺口承接核对（防蒸发）**：§1.2 列出的每个 Medium 缺口在本架构中都必须有承接物——安装包新鲜度 → §3.5 `check-dist-freshness.ps1`；多机配置漂移 → §3.5 `check-machine-config.ps1`（SSH 形态）；守卫模式沉淀 → §3.2 guards.rs；autostart/listener → §3.1/§3.3。无承接物的缺口不允许留在 §1.2。

---

## 3. 五层目标架构

总览：

| 层 | 回答哪类 bug | 工具 | 运行环境 | 触发点 |
|---|---|---|---|---|
| L1 单元 | 纯逻辑/序列化/边界值错误 | cargo test + vitest | 任意（无窗口依赖） | 每次 push/PR |
| L2 守卫 | "看不见的破坏"：源码模式回归、配置漂移、不变量 | 源码区间守卫 + 配置断言测试 | 本机 Windows | 每次 push/PR |
| L3 集成与基准 | 状态组合、IPC 契约、性能回退 | tauri mock runtime + 真机 P99 探针 | 本机 Windows（探针需隔离会话，CV_E2E_ISOLATED 门） | PR + 每夜 |
| L4 E2E | 跨层行为：前端↔IPC↔原生、真实粘贴落点 | playwright-core（扩展现有 maintenance-browser-e2e 模式，webview 层）；UIA 扫描 + paste-probe（原生层） | webview 用例任意环境；原生用例限隔离环境（CV_E2E_ISOLATED 门） | webview 用例每夜 + 发版前；paste e2e 发版前 |
| L5 发布感官门 | 装得上、起得来、人用着对 | smoke-install / release-gate / 人工感官清单 | GH runner（smoke-install，待 PoC 裁决）+ 真机 A/B（只读检查） | tag 时机器项、发布时人工项 |

### 3.1 L1 单元层 —— "这段逻辑算错了吗"

- **工具**：Rust 侧现有 `#[cfg(test)]` 体系（已较成熟，84 passed）；前端引入 **vitest + @vue/test-utils + jsdom**。
- **回答**：纯函数错误、数据边界（NUL、超长 payload、空值）、i18n 文案键缺失、fetch 触发条件漏判（事故 #6）、**确认-armed 状态机漏 disarm（事故 #10，用户数据险毁级）**。
- **交付物与路径**：
  - `src-tauri/src/win/autostart.rs` 内补 `#[cfg(test)]`：把 `set_autostart` 的 RUN 值格式抽出为纯函数（含 `/autostart`、路径加引号规则），守卫断言其格式（autostart.rs:31 的 format! 规则）。
  - `src-tauri/src/lib.rs`：把 global-shortcut handler 的 toggle 决策（`PANEL_VISIBLE ? hide : show`）抽成纯函数 `fn hotkey_toggle(visible: bool) -> Action`，单测覆盖（模式同 commands.rs:54 handshake_child 先例）。托盘菜单项字符串→action 分发同理。
  - `src/modules/openPolicy.js`（新）：从 App.vue:284 抽出 fetch-on-open 决策（`(c.content||'').length >= 4096 || c.text_path` 条件）为纯模块（App.vue:125/141 是 list_clips/stats invoke，非本决策，初版引错行号已修正）；`src/modules/i18n.js`、`src/modules/settingsStore.js`（新）：App.vue:23-66 的 i18n 与 localStorage 逻辑拆出；`src/modules/confirmArmed.js`（新，事故 #10）：清空历史确认的 armed 状态机抽为纯模块，用例以状态转移表穷举断言——**任何**面板关闭/切页/失焦事件都必须 disarm，未 disarm 的转移一律红（CHANGELOG.md:73 的回归用例）。**注意：拆模块是可测性前置改造，本规划仅列为前置条件，实施属代码改动、另行立项。**
  - `src/modules/__tests__/*.spec.js`（新）：vitest 用例。
- **运行命令**：`cargo test`（根，经 .cargo/config.toml 指向 src-tauri/target）；`npm test`（package.json 新增 `vitest run`）。
- **CI 接入点**：ci.yml frontend job 加 `npm test`（现为仅 build，ci.yml:12-18）；rust job 已有 cargo test。

### 3.2 L2 守卫层 —— "事故模式有没有被重新引入"

- **工具**：沿用已验证可行的 `include_str! + 区间 + contains` 源码守卫模式（win/mod.rs:313 证明可稳定工作），**但明确其定位是底线而非证明**（见 §1.3-1）；跨机器的配置断言不属此层，见 §3.5 SSH 脚本。
- **回答**：事故写法回归、硬性规则被破坏（钩子回调禁 I/O、同步 command 禁阻塞窗口创建）。多机配置漂移（事故 #7）由 §3.5 的 SSH 校验脚本回答，不在此层。
- **交付物与路径**：集中在 `src-tauri/src/guards.rs`（新测试模块），每条 ~20 行：
  - `mouse_proc_must_not_do_file_io`：mod.rs:211-221 区间，禁 Open/Read/Write 文件 API 字样（TESTING.md §3-2 硬性规则）。
  - `sync_command_must_not_create_window`：commands.rs invoke 区间，禁 CreateWindow/同步等待（TESTING.md §3-3）。
  - 既有 `hook_pump_must_not_sleep` 保持；行为级替代见 L3。
  - **机器配置守卫不在此层**：热键配置在各机 `%APPDATA%\com.clipvault.app\settings.json`（DEPLOYMENT.md:19/:26），仓库无该文件，cargo 测试在 GH runner 上读不到、在本机只能读到 B 机——事故却发生在 A 机（仅 SSH 可达，cmd 默认 shell）。它以 **`scripts/check-machine-config.ps1` 部署校验脚本**形态放在 §3.5，经 SSH 单行 `powershell -Command` 执行，进部署 SOP 而非 `cargo test`。
  - 既有 `hook_pump_must_not_sleep` 保持；行为级替代见 L3。
- **运行命令**：`cargo test --lib guards`（可单跑守卫子集）。
- **CI 接入点**：随 `cargo test` 进入 ci.yml:36-37 与 release.yml:34-35，无需新增 job。

### 3.3 L3 集成与基准层 —— "组装起来还对吗、有没有变慢"

- **工具**：tauri **mock runtime**（`tauri = { features = ["test"] }` + dev-dependencies，启用 `tauri::test::mock_app()`）；**真机探针**做延迟/时序 P99 测量。**不引入 criterion**——理由：(a) windows-latest runner 硬件不固定，跨 run 绝对耗时对比无统计意义；(b) GetMessageW 阻塞泵空闲时不消耗可测 CPU，微基准测不出输入延迟，无法证明它声称要拦截的事故 #1 的性质（§1.3-1 已承认行为级探针才能证明）。行为证明全部交给真机探针。
- **回答**：IPC 序列化/State 注入错误（当前零覆盖）、16 命令契约回归、剪贴板 listener 时序失败、钩子 pump 延迟回退（事故 #1 的行为级证明）、db 规模回退。
- **交付物与路径**：
  - `src-tauri/tests/ipc_contract.rs`（新，首个集成测试）：mock_app 上 invoke 全部 16 个命令（替换 commands.rs:126-137 的编译期探针错觉）。**无桌面环境归属与副作用门控（必修定义）**：ci.yml 现有裸 `cargo test`（windows-latest）会自动纳入新增的 `src-tauri/tests/*.rs`，因此 ipc_contract **必须设计为无交互桌面可全绿**——这正是 mock runtime 的意义，也是它的 CI 归属（跑 ci.yml，nightly 的 `--lib` 不含集成 target）。落地规则：① 纯读命令（list_clips/stats/get_settings 等）直接真实 invoke；② **有副作用命令（paste_clip 触发 injector/SendInput、show_panel_cmd/create_zoom 创建真实窗口/托盘）在 ipc_contract 里只测参数校验与错误分支**（无效 id / 非法 payload 应返回 Err，走不到副作用路径），副作用路径一律归 L3 探针层（真机隔离门内）；③ 若某命令连校验分支都绕不开副作用，则该命令在 mock 侧降级为类型/签名探针并如实注释。阶段 2 验收标准据此含一条：ipc_contract 在 GH windows runner（无桌面）全绿。
  - `src-tauri/tests/clipboard_listener_probe.rs`（新）：idle-gated（用既有 `clipboard_test_lock` 串行化），写剪贴板断言回调超时内触发 + latest-wins。
  - `src-tauri/tests/hook_latency_probe.rs`（新）：idle-gated 行为探针，测试进程内装 WH_MOUSE_LL，发送带时间戳输入序列，**断言回调间隔 P99 < 阈值**（阈值实测定，建议初值 5ms 并留校准期）。这是对事故 #1 的真证明，比字符串守卫强一个量级。
  - **两个探针与 paste e2e 同用一道 `CV_E2E_ISOLATED=1` 门**（对齐 §1.3-3 与 §3.4）：它们都要发真实输入/写真实剪贴板，在 B 机（生产实例运行中的用户机器）上裸跑既违反 TESTING.md §3-1 禁令、又会被单实例插件（src-tauri/Cargo.toml:21）顶替、还会污染用户剪贴板。门内流程统一：停生产实例 → 按 TESTING.md §6 留存剪贴板 → 跑探针 → 恢复剪贴板 → 重启生产实例。门由 `scripts/run-native-probes.ps1`（新）强制：探针进程启动时检查 env 与父脚本标记，缺门直接 `eprintln!` 拒绝并 exit 非 0。两个探针标 `#[ignore]`。
- **运行命令**：`cargo test --test ipc_contract`；探针在隔离环境经执行器按 **target** 跑：`cargo test --test hook_latency_probe --test clipboard_listener_probe -- --ignored`（由 `scripts/run-native-probes.ps1` 包装并强制隔离门——**不用 `--exact`/按文件名过滤**：`--exact` 匹配完整测试名，用目标名过滤会匹配 0 个测试且 cargo exit 0，静默假绿比跑错更危险，此为初版错误、评审指正后修正）。
- **CI 接入点**：nightly workflow（`.github/workflows/nightly.yml`，新）跑 **`cargo test --lib -- --ignored`**——深度套件（db.rs 的 4 个 `#[ignore]` 基准）全在 lib 单测里，nightly 按 target 选择即可，**不需要也不应该用 `--skip` 过滤**：cargo 的 `--skip/--exact` 匹配的是含模块路径的完整测试名，不是测试目标文件名，按目标名过滤一条也跳不掉，反而会把交互探针拉到无交互桌面的 runner 上假红（初版两版过滤命令均错，评审指正后按 target 选择修正）。探针只在真机/隔离环境按 `--test` 目标跑（见运行命令）。**时序断言门控（必修裁决）**：deep 基准内的 `ratio < 3.0`（db.rs:1253）是跨机时序敏感断言，与本文档裁撤 criterion 的理由（runner 硬件不固定）完全同构——阶段 0 把该基准拆为两个测试：计数断言部分（确定性，进 nightly 门禁）+ ratio 时序部分（保留 `#[ignore]`，仅在真机 release 构建 + 校准机跑，输出打印不卡门禁）。否则 nightly 在慢 runner 上一波抖动即假红，"连续 3 晚绿"不可达成。nightly 各 job **显式钉 `runs-on: windows-latest`**——webview 用例用 `chromium.launch({channel:'msedge'})`（maintenance-browser-e2e.mjs:51）且 playwright-core 不下载浏览器，ubuntu runner 不预装 Edge，跟错 OS 直接红。nightly 深度套件 job 设 `timeout-minutes: 90` 独立 job（全套基准本机 608s，§1.1）。探针永不进 GH Actions。

### 3.4 L4 E2E 层 —— "用户按下去之后到底发生什么"

- **工具**：
  - webview 层：**playwright-core**（已作为 devDependency 存在于 tests/maintenance-browser-e2e.mjs 的模式上扩展，**不引入 WebdriverIO/@wdio/tauri-service**——裁撤理由见头部约束行修订与 §5 第 10 条）。
  - 原生层：UIA 扫描（扩展既有 scripts/uia-dump.ps1 至托盘 flyout）+ paste-probe.ps1 落点断言，**在隔离执行环境跑**（见 §1.3-2 与下方"隔离载体"）。
- **回答**：前端↔后端契约破裂（事故 #6）、粘贴链路端到端失败（事故 #3/#9）、托盘/热键用户入口失效、安装后首启行为。
- **交付物与路径**：
  - `tests/panel-webview-e2e.mjs`（新，复用 maintenance-browser-e2e.mjs 的 node:test + playwright-core + demo-shim 模式）：面板打开/搜索/复制回贴/fetch-on-open 决策，跑编译后 dist。webview 用例跑在 demo-shim 上，**不启动真实 GUI app**，因此可进 nightly。
  - `scripts/paste-e2e-gated.ps1`（新）：paste 链路 e2e 包装器。**仅在隔离门下运行**（env gate `CV_E2E_ISOLATED=1`，与 §3.3 探针同一道门；门内流程：停生产实例 → TESTING.md §6 留存剪贴板 → 跑 → 恢复剪贴板 → 重启**生产实例** → **验证恢复**）。**实例路径显式化**：被停/重启的生产实例是 `F:\Apps\ClipVault\app.exe`（DEPLOYMENT.md B 节，安装路径）；e2e-paste.sh 操作的是 `src-tauri/target/debug/app.exe`（scripts/e2e-paste.sh:6 debug 构建）——四步流程必须按路径区分两者，禁止把 debug 探针实例当生产实例留着。**恢复失败兜底**：最后一步不是"重启"而是"重启并验证"——启动失败重试 3 次，仍失败则立即告警（脚本 exit 非 0 + 输出显式 RED 标记），执行者必须在 checklist/MAINTENANCE_LOG 回写"生产实例未恢复"，禁止静默收场（B 机生产实例 autostart=true 也不能指望下次重启自愈——用户当会话剪贴板工具已消失）。**授权红线**：停生产实例属破坏性操作，对齐 DEPLOYMENT.md 隐私红线的"用户明示"标准——执行窗口（无人在场时段）须事先获用户明示批准并记入 MAINTENANCE_LOG，不允许脚本自行择时。对用户真实桌面保持 TESTING.md:31 禁令不变。
  - `scripts/uia-tray-probe.ps1`（**可选，降级**）：托盘点击→面板 show 的 UIA 断言。评审裁定降级——Windows 托盘图标 UIA 自动化受"图标是否折叠进溢出区"影响极脆，为一条断言单独养脚本对一人+AI 维护成本高于收益；默认归入 RELEASE_CHECKLIST §8 人工感官清单，仅当未来发现自动化收益时再启用。
- **运行命令**：`node tests/panel-webview-e2e.mjs`；`CV_E2E_ISOLATED=1 scripts/paste-e2e-gated.ps1`（隔离门内，内部调 e2e-paste.sh + paste-probe.ps1）；`scripts/uia-tray-probe.ps1`。
- **隔离载体（回应"载体悬空"评审点，如实陈述）**：Windows 上脚本化创建第二个可交互会话**基本不可行**——fast user switching 无法无人值守切入、RDP 断开后合成输入落在锁屏桌面、VM 则要求在生产 B 机上装虚拟化。因此采纳降级形态为**默认方案**：`CV_E2E_ISOLATED` 门不依赖第二会话，语义为"停生产实例 + 剪贴板留存恢复 + 限定时间窗口"的在位隔离（同一交互桌面、生产实例已停、剪贴板已保护）；`setup-isolated-session.ps1` 降级为可选升级项（未来若有第二台备用机或 VM 再评估），**不是 paste e2e 的前置条件**。降级形态的残留风险如实声明：无法隔离"同一桌面上的其他用户操作"，故 paste e2e 必须在无人在场的执行窗口跑。
- **CI 接入点**：nightly.yml 跑 webview 用例（demo-shim，无 GUI 依赖）；paste e2e 与 UIA 探针**不在 GH Actions**（需交互桌面），由发版前人工/计划任务在 B 机执行，结果回填 checklist。

### 3.5 L5 发布感官门 —— "装得上、起得来、人用着对"

- **工具**：PowerShell 冒烟脚本 + GH Actions 步骤 + 人工感官清单（RELEASE_CHECKLIST §8 保持人工，是**有意的人类门禁**）。
- **回答**：安装器断裂（事故 #4）、构建配置静默丢失（事故 #5）、带病发布/误公开（事故 #8 两个半边）、多机状态漂移（事故 #7）、安装包内容过期（§1.2 Medium 缺口，RELEASE_CHECKLIST §3）。
- **交付物与路径**：
  - `scripts/check-versions.ps1`（新）：五处版本一致性机器校验（package.json / src-tauri/Cargo.toml / Cargo.lock / tauri.conf.json / CHANGELOG.md），把 release.yml 现在的 1/5 补到 5/5。**CHANGELOG 解析规则须先定义**：CHANGELOG 同时存在 `[0.1.7]`、`[Unreleased]`、`[0.1.5] Unpublished` 段（CHANGELOG.md:8/:15/:31），"当前版本" = **最新的非 Unreleased、非 Unpublished 的已发布段**，脚本按此规则提取后再与四处比对，否则五处必然对不齐。
  - `scripts/check-build-profile.ps1`（新）：守卫事故 #5，**不做 PE 段表手解/大小上界**（自造轮子且上界每次构建都漂）。改为两条廉价断言：① 源码守卫——`[profile.release]` 只允许出现在根 Cargo.toml、不出现在 src-tauri/Cargo.toml（现状根 Cargo.toml:8-14 已有注释锁住该规则，脚本只是机器化它）；② release 构建后跑 `cargo build --release -v`（或读取构建日志）断言输出含 lto/strip 相关 flag。①可同时以 guards.rs 守卫测试形态存在（§3.2 模式），脚本层只做②。
  - `scripts/check-dist-freshness.ps1`（新）：脚本化 RELEASE_CHECKLIST §3，**只做时间戳断言**（dist 新于 bundle 产物）；"关键修复字符串 grep"留人工清单——脚本化它反而新增每版手工输入点（评审建议采纳）。
  - `scripts/smoke-install.ps1`（新）：静默安装→启动→进程确认→卸载，脚本化 RELEASE_CHECKLIST §4 中 LastWriteTime/StartTime/进程确认项（§8 感官项仍人工）。**执行环境裁决（必修，如实声明）**：RELEASE_CHECKLIST.md:23 明文"本仓库正式候选包唯一来源是 GitHub Release CI；Windows B 当前环境禁止本地跑"——初版所写"打包机或隔离环境"中，打包机**不存在**（B 机被禁止本地 build），隔离环境**未解决**（§3.4）。因此唯一候选环境是 **GH runner**，而 runner 能否承载带 tray-icon + single-instance + POSTINSTALL 钩子的 GUI app **未验证**。落地方式：阶段 1 先做一个 runner PoC（release.yml 手动触发，在 windows runner 上 `npx tauri build` 产物静默安装→启动→确认→卸载）；PoC 失败则**如实降级**：事故 #4 的机器拦截层标记为"当前无执行环境"，安装验收回落到发版后 A/B 机只读检查（进程确认 + cv.log + check-machine-config），并把"搭建可安装测试环境（VM/第二台机）"记为待办而非假装门禁存在。**A 机禁区**：本脚本是破坏性操作（卸载+重装+启动），A 机（生产主力机 DESKTOP-CD2C03F，v0.1.6 运行中，DEPLOYMENT.md:5-8）只跑只读检查（进程确认、`check-machine-config.ps1`、cv.log grep），经 SSH 单行 `powershell -Command` 调用（cmd 默认 shell 兼容）。SSH 校验模式不得外推到安装/卸载。**回滚预案**：任何环境实装 draft 包前，先备份 `%APPDATA%\com.clipvault.app\`（对齐 DEPLOYMENT.md 既有备份惯例），卸载后重装当前 v0.1.6 安装包并验证进程/热键/心跳恢复。
  - `scripts/release-gate.ps1`（新）：**事故 #8 的缺陷半边**——发布前断言无未闭环 P1（读 CHANGELOG/issue 标记）；**公开半边**——draft→published 状态写后读验证（RELEASE_CHECKLIST §7 第 3 条的机器化）。§7 其余 3 条（禁程序化提交、draft 变更只走认证 API、单独 GO 授权）**无法用脚本完全拦截，如实承认**：保持 checklist 人工签核，release-gate 只在产出的 draft 上做状态回读收窄误公开窗口。
  - `scripts/check-machine-config.ps1`（新）：**事故 #7 的承接物，部署后检测层（非安装时阻止层，如实降级表述）**——热键继承发生在安装/POSTINSTALL 时点，本脚本只能**事后发现**漂移，不能防住安装瞬间；映射表中该层定位为检测。形态：**部署时经 SSH 执行，不在 cargo/CI 里**。取数方案（评审建议采纳，避免 A 机读 SMB 仓库文件）：SSH 单行 `powershell -Command` **把期望值作为参数从 B 机仓库的 `docs/machine-registry.json` 传入**，A 机侧只读本地 `%APPDATA%\com.clipvault.app\settings.json` 并回传实测值 JSON，**对比在 B 机侧完成**——A 机不需要读 K$ 共享，规避 Git Bash→cmd 双跳的复杂转义（A 机默认 shell 为 cmd，DEPLOYMENT.md:7）。登记值模型（不是硬编码白名单：热键是用户偏好，DEPLOYMENT.md:19 记录 2026-10-30 应用户要求由 Ctrl+Alt+V 改回 Alt+V，硬编码会在用户下次改键时立即假红）：`docs/machine-registry.json`（新，`{machine, hotkey, autostart, panelHeight}`，变更偏好时 SOP 要求同步登记，登记滞后视为流程缺陷而非脚本缺陷）。实跑骨架先行：阶段 1 验收前先在 A 机跑通一条最小 SSH 单行（读 settings.json 回传），验证转义可行后再定稿完整脚本。
- **运行命令**：各自 `powershell -File scripts/<name>.ps1`；check-machine-config 经 SSH：`ssh a-machine "powershell -Command <单行校验>"`。
- **CI 接入点**：check-versions、check-build-profile、check-dist-freshness、release-gate 进 release.yml tag 流程（前三者 build 前后各按其语义排位，release-gate 在 draft 产出后、发布动作前）；smoke-install 依阶段 1 PoC 结论决定是否进 release.yml（失败则不进，见 §3.5 执行环境裁决）；check-machine-config 不进 CI（目标机不在 runner 上），进 DEPLOYMENT.md 部署 SOP。

---

## 4. 四阶段落地路线

### 阶段 0：止血与消红（半天～1 天）

- 裁决 deep_10k_vs_100k_scale_bench，**两件事分开做**（2026-10-31 本会话实跑复核：失败在 db.rs:1154-1156 计数断言 9700 vs 9500，0.75s 确定性失败，非时序）：① 修计数回归根因——应 dedup 的 200 条 upsert 未去重，确认 dedup/latest-wins 语义哪边正确，修代码或修断言；② 把基准内时序断言 `ratio < 3.0`（db.rs:1253）拆出为独立 `#[ignore]` 测试（真机 release + 校准机跑，打印不卡门禁）——否则 nightly 门禁会因慢 runner 假红，与本文档裁撤 criterion 的逻辑自相矛盾。**不消红则阶段 2 的 nightly 门禁会把红带进门禁。**
- 把 tests/maintenance-browser-e2e.mjs 接入 `package.json`（`npm run test:e2e:demo` = `vite build && node tests/maintenance-browser-e2e.mjs`——脚本读 dist/ 静态产物（maintenance-browser-e2e.mjs:22），**必须先 build**，否则首次运行 404 全红）并写入 docs，消灭孤岛。
- **验收标准**：`cargo test -- --ignored` 全绿（计数断言在门禁内、ratio 断言已拆出）；`npm run test:e2e:demo` 有入口可跑（含 build 前置）。
- **怎么算没退化**：这两个命令成为 checklist 固定项；再出现 `#[ignore]` 红测试即视为事故级问题。

### 阶段 1：守卫扩散 + 版本/发布机器门禁（2～3 天，拦截事故 #4/#5/#7/#8）

- 落地 L2 守卫（guards.rs：mouse_proc 禁 I/O、sync command 禁建窗、profile 仅在根 Cargo.toml）+ L5 全部脚本（check-versions / check-build-profile / check-dist-freshness / smoke-install / release-gate / check-machine-config）+ `docs/machine-registry.json` 登记文件 + release.yml 新步骤 + DEPLOYMENT.md SOP 增补 SSH 只读校验条目 + **smoke-install 的 GH runner PoC**（手动触发的 release.yml 变体：runner 上装 draft 包→启动→确认→卸载；见 §3.5 执行环境裁决——初版所写"打包机"不存在，RELEASE_CHECKLIST.md:23 明文候选包唯一来源是 GH Release CI）。
- **验收标准**：`cargo test --lib guards` 绿；故意改错一处版本号，tag 构建被 check-versions 拒（演练一次）；故意在 mouse_proc 区间加文件 I/O 字样，守卫测试变红（演练一次）；故意把 `[profile.release]` 移回 src-tauri/Cargo.toml，守卫变红（演练一次）；check-machine-config.ps1 经 SSH 对 A 机**先实跑最小骨架**（单行读 settings.json 回传，验证 Git Bash→cmd 转义可行）再定稿，完整脚本实跑输出"实测值 == 登记值"并留验收记录；smoke-install runner PoC 有明确结论（成/败都记入 MAINTENANCE_LOG，失败则按 §3.5 如实降级并记待办）。
- **怎么算没退化**：release.yml 的机器步骤不可被 `continue-on-error` 绕过；任何新"不可见破坏"类规则（TESTING.md §3）新增时**必须**同步落一条守卫，否则不许合入规则文本；每次部署 SOP 必须含 check-machine-config 一步；改用户偏好必须同步 machine-registry.json，否则守卫假红视为登记流程缺陷而非脚本缺陷。

### 阶段 2：集成 + mock runtime + nightly 门禁（3～5 天，拦截事故 #1 行为级 + 契约回归）

- 启用 tauri test feature + dev-dependencies；落地 `tests/ipc_contract.rs`（按 §3.3 副作用门控规则：副作用命令只测校验/错误分支）；落地 hook_latency_probe 与 clipboard_listener_probe（含 CV_E2E_ISOLATED 门 + run-native-probes.ps1 隔离执行器，按 `--test` 目标跑探针）；建 nightly.yml（全 job 显式 `runs-on: windows-latest`）跑 `cargo test --lib -- --ignored`（深度套件独立 job，`timeout-minutes: 90`）+ webview 用例（panel-webview-e2e.mjs，demo-shim 无 GUI 依赖）。
- **验收标准**：16 个 IPC 命令按副作用门控规则各有真实 invoke 断言，且 **ipc_contract 在 GH windows runner（无交互桌面）经 ci.yml 裸 `cargo test` 全绿**——做不到全绿的命令按 §3.3 规则降级为校验分支/类型探针并注释；hook_latency_probe 在隔离门内输出 P99 数值并校准出稳定阈值；run-native-probes.ps1 缺 `CV_E2E_ISOLATED` 时拒绝执行（演练一次）；nightly.yml 连续 3 晚绿。
- **怎么算没退化**：nightly 红≥1 晚需在当日记 MAINTENANCE_LOG 并说明；nightly 深度套件命令为 `cargo test --lib -- --ignored`、探针为 `--test` 目标选择——`--ignored --skip <目标名>` 或 `--ignored --exact <目标名>` 形态属门禁配置错误（匹配不到测试名，假红或假绿，历史两版已证伪）；删除/绕过探针隔离门视同破坏门禁；不新增任何跨 runner 的微基线对比（无统计意义，见 §3.3 工具行）。

### 阶段 3：前端可测性 + E2E 隔离合规化（5～8 天，含代码拆分前置改造，拦截事故 #3/#6/#9）

- vitest 落地 + App.vue 拆出纯模块（openPolicy/i18n/settingsStore/**confirmArmed**，事故 #10 用例）；panel-webview-e2e.mjs（playwright-core + demo-shim）落地 webview 用例；在位隔离门跑通一次 paste e2e 并留证据（含生产实例恢复验证、剪贴板恢复、用户明示授权与执行窗口记录）。
- **验收标准**：`npm test` 绿且 openPolicy 覆盖 text_path 分支（事故 #6）、confirmArmed 覆盖全部 disarm 转移（事故 #10）；隔离门内 paste e2e 输出 cv-probe.txt 落点断言通过，且跑完后 `F:\Apps\ClipVault\app.exe` 生产实例经"重启并验证"步骤断言恢复运行、用户剪贴板内容恢复（TESTING.md §6 闭环，事故 #3/#9 的回归证据）。
- **怎么算没退化**：TESTING.md:31 禁令未被放松——paste e2e 只在 `CV_E2E_ISOLATED=1` 下可跑，脚本缺该 env 直接拒绝执行；每次发版 checklist 必须附隔离门内 paste e2e 的一次运行证据（取代 "awaits user manual run" 状态）；run-native-probes.ps1 的"停实例（按 F:\Apps 路径）→留存→恢复→重启（按 F:\Apps 路径）"四步缺一即为门禁缺陷。

> 阶段 3 的 App.vue 拆分是代码改动，超出"只写文档"的本任务范围，此处仅为规划；实施前需单独评审。

---

## 5. 明确不做的事（避免过度工程）

1. **不引入任何重型商业工具**：无 TestComplete/Ranorem/付费 UIA 套件；原生层用现成 PowerShell + UIA + Win32 探针。
2. **不做覆盖率数字指标**：不设"覆盖率必须 X%"目标——本项目一个人+AI，覆盖率数字会诱导写弱断言测试（现状已有的编译期探针错觉就是教训，commands.rs:126-137）。以"事故映射表里每条有拦截者"为准。
3. **不解除用户桌面合成输入禁令**：TESTING.md:31 保持不变；隔离环境是唯一合规路径，不为省事加白名单例外。
4. **不追求把交互依赖测试塞进 GH Actions**：windows runner 无交互桌面，强塞只会得到假绿。交互探针固定在真机/隔离会话跑。
5. **不做跨浏览器/多分辨率矩阵**：目标平台是 Windows 双机 webview2，webview 用例只跑 Edge WebView2 单引擎（playwright-core channel=msedge）。
6. **不引入 Snapshot/视觉回归工具**：面板 UI 简单，§8 人工感官清单已覆盖，自动化截图比对维护成本大于收益。
7. **不重写现有 84 个单测**：现有质量较高（具体值断言+回归注释），只补缺口不推倒。
8. **不做混沌/故障注入框架**：listener 失败分支等用 20 行定向注入即可，不建平台。
9. **不做跨 runner 微基准与基线 artifact 管线**（评审裁撤 criterion 后追加）：windows-latest 硬件不固定使绝对耗时对比无统计意义； GetMessageW 阻塞泵空闲不耗可测 CPU，微基准测不出输入延迟。一个人+AI 维护 nightly 基线对比管线的成本大于收益，性能回退全部由真机 P99 探针门禁。
10. **不引入 WebdriverIO / @wdio/tauri-service**（评审裁撤后追加）：为 3-4 条 webview 用例引入整套 driver 工具链（wdio.conf.js、msedgedriver 版本绑定、tauri-driver 路径），而 @wdio/tauri-service v1.5.1 与 Tauri 2.11 + WebView2 的兼容性未验证、兼容失败的降级方案（playwright-core）本就存在且工作（tests/maintenance-browser-e2e.mjs）——期望收益为负。webview 层直接扩展现有 playwright-core 模式，架构不变、零新依赖。

---

## 附：已知未验证事项（如实声明）

- 84 个测试中有多少能过 GitHub Actions windows runner：**本会话未验证**（未触发 CI）。nightly.yml 建立后首晚运行即回答此问题，预期少量真窗口测试需按 runner 环境门控（`#[cfg(windows_interaction_available)]` 模式或运行时探测跳过）。
- hook_latency_probe 的 P99 阈值：规划给的是校准流程而非拍定数值，避免写出第一次跑就红的门禁。
- App.vue 拆模块、autostart/hotkey 决策函数抽取均为代码改动，本文档仅规划，未实施。
- **隔离载体状态（如实声明）**：初版把 `setup-isolated-session.ps1`（测试账户/VM 快照）当作 paste e2e 前置条件，评审指出 Windows 上脚本化第二交互会话基本不可行（fast user switching 无法无人值守、RDP 断开合成输入落锁屏、VM 需在生产机装虚拟化）——**该路线属"方案未解决"而非"待验证"**。修订后默认方案为在位隔离（§3.4），第二会话/第二台机为可选升级项，不阻塞阶段 3 验收。
- **smoke-install 执行环境未解决（阶段 1 PoC 裁决）**：RELEASE_CHECKLIST.md:23 明文候选包唯一来源是 GH Release CI、B 机禁止本地 build——初版所写"打包机"**不存在**，唯一候选环境是 GH runner，而 runner 能否承载带 tray-icon + single-instance + POSTINSTALL 钩子的 GUI 应用未实测。阶段 1 以 PoC 裁决（见 §3.5）：成功则进 release.yml；失败则事故 #4 的机器拦截层如实降级为"当前无执行环境"，安装验收回落发版后 A/B 机只读检查，"VM/第二台机测试环境"记待办。任何环境实装 draft 包前必须备份 `%APPDATA%\com.clipvault.app\` 并备好 v0.1.6 安装包回滚。
- 证据勘误记录：初版 §1.1 曾写"`ls src-tauri/tests` → 不存在"，实况为目录存在但为空（2026-09-19 创建，`ls -la` 核实）；结论（无集成测试）不变，引用已修正。初版至三版的其余勘误：机器配置守卫从 cargo 集成测试改为 SSH 脚本形态，且从"拦截层"如实降级为"部署后检测层"（§2 #7、§3.5）；criterion 基准裁撤（§3.3、§5-9）；WDIO 线裁撤、改 playwright-core（§3.4、§5-10）；nightly 探针过滤命令两版均错（`--skip`/`--exact` 按目标名匹配不到测试名），改为按 `--test` target 选择 + 深度套件走 `cargo test --lib`（§3.3、阶段 2）；映射表补入事故 #10（armed clear-history，CHANGELOG.md:73）；deep 基准失败点修正为 db.rs:1154-1156（计数断言，2026-10-31 本会话实跑复核），"db.rs:1171"为函数起点、"608s"为全套基准耗时——初版将两者混写为单条失败的特征；App.vue fetch-on-open 决策行号修正为 :284（初版误引 :125/:141）；B 机生产实例路径引 DEPLOYMENT.md B 节安装路径（初版误引 :26）。三版评审曾质疑"深基准红的证据与代码对不上"，经本会话实跑 `cargo test --lib deep_10k -- --ignored` 复现：**红是真实的**，失败文本与初版记载一致，仅行号归属需修正。
