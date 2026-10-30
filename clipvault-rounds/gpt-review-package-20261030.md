# ClipVault 发版审查材料（投 GPT 终审）— 2026-10-30

- **发布物**：v0.1.6（draft release id=428811926，`draft:true` 未 publish）
- **结论请求**：请 GPT 给出 **GO / NO-GO**
- **硬性条件**：assets 的 SHA256 与 SHA256SUMS.txt 逐项一致后，才可执行安装/发布
- **材料来源标注**：〔执行记录〕= 本轮自动化执行留档（由轮次交接口径提供，本汇总步未重跑）；〔本会话核验〕= 本汇总会话实际执行命令并读取到的结果；〔引用 10-28 档案〕= `clipvault-rounds/2026-10-28-owner-convergence.md` 的既载结论

---

## 1. 版本与发布物状态

〔执行记录，未在本汇总步重跑网络轮询〕

| 项 | 值 |
|---|---|
| releaseTag | v0.1.6 |
| draft release | id=428811926，`draft:true`（未 publish，红线3 遵守） |
| assets（三类齐全） | `ClipVault_0.1.6_x64-setup.exe`（2,355,846 B）、`ClipVault_0.1.6-portable.exe`（7,916,032 B）、`SHA256SUMS.txt`（193 B） |
| Release workflow | run #54，id=41027499367，conclusion=success（含版本断言 v0.1.6==tauri.conf.json、fmt/clippy/test 门禁） |
| 轮询方式（红线5） | `curl --ssl-no-revoke` + `$(gh auth token)`，间隔 60s，第 5 次轮询（约 5 分钟）DONE |
| v0.1.5 draft | id 423764142，原样保留 `draft:true` 未动 |

**版本五处一致〔本会话核验〕**：执行 `grep -n '"version"' package.json src-tauri/tauri.conf.json` + `grep -n '^version' src-tauri/Cargo.toml` + `grep -n '^name = "app"' -A1 Cargo.lock` + `head -20 CHANGELOG.md`：

1. `package.json:4` → `"version": "0.1.6"` ✅
2. `src-tauri/tauri.conf.json:4` → `"version": "0.1.6"` ✅
3. `src-tauri/Cargo.toml:3` → `version = "0.1.6"` ✅
4. `Cargo.lock:63-64` → `name = "app"` / `version = "0.1.6"` ✅
5. `CHANGELOG.md` → `## [0.1.6] - pending release` 条目在位 ✅

**仓库状态〔本会话核验〕**：`git log --oneline -8` 显示 HEAD=`1f8b596 release: v0.1.6 (paste-acceptance round: zoom oversized-text fetch, installer auto-launch fix, identifier-isolated builds)`；`git status --short` 为空（工作树 clean）。

**一处如实记录的差异**：执行记录称前端产物在 `dist/`（2026-10-28 19:58:35）；本会话 `ls -l --time-style=full-iso dist/` 实测 `index.html`/`assets/` mtime 为 **2026-10-30 16:21:38**（应为本轮 identifier 隔离构建/本地验证时重建）。不影响 CI 从 tag 构建产物的正确性，如实登记。

---

## 2. 四场景粘贴验收（2026-10-30 08:00–08:01 UTC 轮）〔执行记录〕

```
textbox:      2026-10-30T08:00:50.316Z PASS=true
richtextbox:  2026-10-30T08:01:00.564Z PASS=true
notepad:      2026-10-30T08:01:38.445Z PASS=false（环境失败）
chrome:       2026-10-30T08:01:48.264Z PASS=true
```

**结果：3/4 场景最新 PASS=true；notepad 为测试环境失败（非产品失败）。**

### 2.1 notepad 场景失败定性（环境失败证据链）

- focus-take take#0 成功 + UIA 焦点回读 `FOCUSED:VERIFIED:文本编辑器` + 注入器 `focus matched` 后 Ctrl+V 已发，**编辑器内容仍为 BEFORE**；今日 4 轮 0 落地。
- 根因定性：红线禁止合成输入下，harness 无手段复现用户等效 XAML 焦点——键被 WinUI3 记事本吞掉。
- 产品 fail-closed 契约**全链正确执行**的证据：
  - 真实目录 `%APPDATA%/com.clipvault.app/cv.log` 07:14Z 轮：`paste: sending ctrl+v handshake target=310f88 focus=0` → `[cv-inj] signaled want=(3215240,0)` → `focus matched fg=3215240 focus=14355572` → `sent`；
  - 本轮 08:01Z 隔离 cv.log 同链路，且 UIA 焦点回读 VERIFIED。
- 即：粘贴链路产品侧行为正确（焦点校验→握手→发送），失败发生在目标应用消费键的环节。

### 2.2 本轮 harness 修复（仅限 tmp-ux/，未触产品代码）

- `driver.mjs`：take 重试 3×800ms → **10×2500ms**（上轮 9 次全部败于 UWP 设置窗口 ApplicationFrameHost pid 12696 霸占前台 ~60s）
- `focus-notepad.ps1`：增加焦点回读验证
- `idle-watcher.ps1`：增加真实 exe 孤儿注入器清扫（`FindWindowW 'ClipVaultListener'` 类名匹配，非 identifier 作用域）、测试实例 APPDATA 重定向、driver 超时 240s→700s、**移除 teardown 中危险的共享记事本窗口 UIA close**（对应 §3.1 事故）
- 重布防一次已用：WMI 脱离进程树 PID 54292，16:00:14 触发，16:01:56 COMPLETE

### 2.3 收尾核验〔执行记录〕

- 真实实例 `F:\Apps\ClipVault\app.exe` pid 13152 运行中；`%APPDATA%/com.clipvault.app/cv.log` 最新行 `1793347822660 [cv] hotkey registered: Alt+V`
- watcher / 目标窗口 / 残留 edge 均为 0
- 未做：draft release 状态未触碰（红线3）；该验收轮未改任何产品代码（红线5 未涉 API 调用）

---

## 3. 本轮须上报三项（如实，无隐瞒）

### 3.1 事故：共享记事本窗口 UIA close 误伤用户脏标签

我的诊断式 UIA close 在共享记事本窗口（pid 32116）上丢弃了我们目标的脏标签，窗口切换到用户标签；第二次运行**误将用户 `zzz_chg_0025.txt` 脏标签的"不保存"Invoke**（未保存缓冲已丢，TabState 无今日备份）。止损：已删除诊断脚本，并在 watcher 中**永久移除该 close 路径**。

### 3.2 失误：teardown 用 Remove-Item 而非还原 APPDATA

重布防轮 teardown 误用 `Remove-Item` 而非还原 APPDATA，恢复的真实实例 **16:01:52–16:07:37 无 APPDATA 运行**（cvlog 丢弃）。已发现并以正确环境重启修复；watcher 已改为保存/还原 `$realAppdata`，语法 PARSE OK。

### 3.3 隔离真相：历史"测试实例"一直在写真实数据目录

- release exe（10-28 20:03 构建）identifier 即 `com.clipvault.app`（**红线4 保持未动**）；历史所谓"测试实例"一直在写真实数据目录。
- APPDATA 环境变量重定向只隔离 cvlog + 注入器握手；db 走 Known Folder **仍写真实目录**（本轮新增 row 895 为证）。
- **红线2 清理已执行**：整目录备份至 `C:/Users/13397/cv-iso-test/backup-real-appdata-20261030-160955`（含 wal）→ 精准删除 4 条测试行 rowid=846,847,848,895 → `wal_checkpoint(TRUNCATE)` 返回 `0|0|0` → 复核 CV-ISO 行=0。

---

## 4. 架构排查成果：修复 8 条、暂缓 1 条

### 4.1 修复清单〔执行记录；关键文件本会话核实存在〕

1. **QA 门禁根因**：`cargo fmt` 从仓库根运行但根目录无 Cargo.toml。修复：新增根虚拟 workspace（`members=["src-tauri"], resolver=2`），使 fmt/metadata/test/clippy 全部可从根目录运行。〔本会话核验：`Cargo.toml` 存在，内容与描述一致〕
2. **保持构建产物路径不变**：新增 `.cargo/config.toml` `[build] target-dir="src-tauri/target"`，e2e-paste.sh 的 exe 路径、NSIS 打包路径与既有增量缓存不受 workspace 化影响（实测根目录未生成 target/，`cargo test` 从根运行 1.0s 复用缓存）。〔本会话核验：`.cargo/config.toml` 存在，内容一致〕
3. **Cargo.lock 随 workspace 迁移至仓库根**（git 已跟踪的 `src-tauri/Cargo.lock` 移动），保留锁定依赖图；`cargo metadata --no-deps` exit=0 无警告。〔本会话核验：`src-tauri/Cargo.lock` 不存在、根 `Cargo.lock` 存在（130,734 B）〕
4. **`[profile.release]` 原样搬至根 Cargo.toml**（cargo 明确警告非根 member 的 profiles 会被忽略，不搬会让 release 的 opt-level/lto/strip **静默失效**）；`src-tauri/Cargo.toml` 留注释说明。〔本会话核验：根 Cargo.toml 含 `opt-level="s", lto="fat", codegen-units=1, strip=true`〕
5. **cargo fmt** 全量格式化本轮 8 处违规（capture.rs/commands.rs/db.rs/lib.rs 等），`cargo fmt --check` 从根目录通过。
6. **clippy -D warnings 四处修复**（`docs/RELEASE_CHECKLIST.md` 载明这是发布门禁〔本会话核验：第 18-19 行原文 `cargo clippy --all-targets -- -D warnings`〕）：
   - db.rs `get_clip` 四元组返回类型 type_complexity → 新增 `pub type ClipPayload` 别名
   - lib.rs watchdog 对 `Option<Instant>`（Copy）的 `.clone()` → 改 `*` 解引用
   - commands.rs `paste_clip_future` 编译期守卫加 `#[allow(clippy::manual_async_fn)]`（显式 impl Future + Send 正是被测的 Send 可派生断言，故意不用 async fn）
   - clipboard.rs 测试模块位于 `write_clipboard_text` 之前触发 items_after_test_module → 移至文件末尾
7. **间歇性失败修复（81 passed/1 failed）**：`clipboard_atomicity_tests::failed_png_write_never_destroys_the_previous_clipboard` 与并行测试争用真实系统剪贴板。新增 `win::clipboard_test_lock()`（cfg(test) Mutex）串行化 4 个写真实剪贴板的测试：`win::tests::png_roundtrip_via_clipboard`、`formats::file_tests::files_roundtrip_via_clipboard` 与 `png_roundtrip_via_clipboard`、`clipboard::failed_png_write_never_destroys_the_previous_clipboard`
8. **文档同步**：NOTICE.md:8（`src-tauri/Cargo.lock` → `Cargo.lock (workspace root)`）与 docs/RELEASE_CHECKLIST.md:12。〔本会话核验：NOTICE.md:8 原文 `Cargo.lock (workspace root)` 在位；RELEASE_CHECKLIST.md 第 11-14 行门禁命令清单为 `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test` / `cargo test -- --ignored` / `npm run build`〕

### 4.2 新增测试

无新增测试（本轮为门禁修复：workspace 发现性、格式化、clippy 清洁与测试隔离）；**既有 82 个测试全部保留并通过**〔执行记录〕。

### 4.3 暂缓清单（1 条）

| where | what |
|---|---|
| `src-tauri/src/commands.rs:256-266` | 面板定位纠偏路径把换算系数写死回退为 1.25（`w.scale_factor().unwrap_or(1.25)`），且用窗口"当前所在"显示器的 scale_factor 去除物理目标坐标——跨显示器移动（OS 与 Tauri 记账分歧的高发场景）时系数可能是旧屏的，纠偏可能越修越偏。仅影响错误分支，偏移为外观级 |

---

## 5. QA 门禁

〔执行记录〕：`cargo fmt` / `clippy -D warnings` / `cargo test` / npm 前端构建 **全部通过（三轮内收敛）**。本汇总步**未重跑**这四项命令（82 测试结果、fmt/clippy 清洁均来自执行记录）。

npm 脚本核实〔本会话核验〕：`K:/Project/1-Active-Projects/clipvault/package.json` scripts 段原文仅 `"dev": "vite", "build": "vite build", "preview": "vite preview"`，**不存在 build:web 之类的别名**；唯一前端构建脚本名为 `build`。

---

## 6. 红线遵守情况（单列一节）

| 红线 | 遵守情况 |
|---|---|
| **红线1 合成键鼠禁用** | ✅ 本轮零合成键鼠落桌。notepad 场景在红线禁止合成输入下无 harness 手段复现用户等效 XAML 焦点 → 如实报 PASS=false，4 轮 0 落地未粉饰。继承〔引用 10-28 档案〕结论：合成输入路径在真实桌面不可控（4 次 fail-safe 正确中止），**永久弃用** |
| **红线2 真实库保护** | ✅ 且已纠错：披露历史"测试实例"写真实目录的隔离真相（§3.3）；先整目录备份（含 wal）→ 精准删除测试行 846/847/848/895 → `wal_checkpoint(TRUNCATE)`=0\|0\|0 → 复核 CV-ISO 行=0；真实实例恢复正常运行（Alt+V 心跳） |
| **红线3 draft 不 publish** | ✅ v0.1.6 draft id=428811926 与 v0.1.5 draft id=423764142 均保持 `draft:true` 未动；publish 仅在 GO 之后执行 |
| **红线4 identifier 未动** | ✅ release exe identifier 即 `com.clipvault.app`，保持未动；隔离改由 identifier 隔离构建（`com.clipvault.test`）承担〔引用 CHANGELOG Internal 节：build.rs 导出 CV_IDENTIFIER，数据目录/log/握手通道均跟随 identifier〕 |
| **红线5 不触碰发布外 API/产品代码** | ✅ 验收轮未改任何产品代码、未涉 API 调用。门禁修复轮的代码改动即提交 `179cc56`（QA 门禁根因+粘贴路径加固）与 `1f8b596`（release v0.1.6），全部经 QA 门禁验证后提交 |

---

## 7. 连续性与历史结论引用

**已读档案〔本会话核验〕**：`clipvault-rounds/2026-10-28-owner-convergence.md`（完整读取）——以下结论直接引用，不重复研发：

- **PNG 真实来源捕获【PASS】**（10-28 轮）：测试 identifier 构建（com.clipvault.test 隔离 profile），P/Invoke 以注册格式 "PNG"（fmt=49358）写入 96x40 虚构 PNG，无 DIB、无文本伴随 → 存储行 `[Image 96x40]`、落盘 PNG 结构合法、缩略图生成；走修复后专用分支，非 DIB 回退
- **数据保留【PASS】**（10-28 轮）：隔离 profile 3 行跨实例重启完好；真实 db 全程未动
- **v0.1.4 事故教训**：误发布后平台限制无法转回 draft，body 已加固警告，Latest 保持 v0.1.3，暴露下载 5 次（10-27 取证）→ 本次坚持 draft:true、GO 后才 publish
- **隔离矩阵穷尽结论**（10-28 增补）：Windows Sandbox（未启用）/Hyper-V/VirtualBox/VMware（未装）/APPDATA 环境变量隔离（Known Folder 失效）均不可用；测试 identifier 构建 + CDP 面板内可信点击为可行隔离路径
- **10-29 恢复轮根因链**：`ABORT foreground != target`=测试环境失败（前台锁定策略）→ `focus-take.ps1`（AttachThreadInput+SetForegroundWindow，窗口管理 API 非键鼠注入）；`landed=false 但 paste sent`=宿主消息泵缺失；textbox/richtextbox 于 13:13 轮真实粘贴 PASS

**`gpt-review-package-20261029.md`：不存在**〔本会话核验：`ls clipvault-rounds/` 仅含 2026-10-27 两份、2026-10-28-owner-convergence.md、manual-acceptance-guide-2026-10-28.md、sandbox-acceptance-kit/〕——本文件为实际投审的首份 GPT 审查材料。

---

## 8. 门禁状态表（对照 10-28 档案格式更新）

| 门禁 | 状态 |
|---|---|
| 代码与 tag 一致 | ✅ HEAD `1f8b596`=release v0.1.6，工作树 clean；版本五处一致〔本会话核验〕 |
| cargo fmt / clippy / test | ✅ 执行记录：全部通过（三轮内收敛），82 tests；本汇总步未重跑 |
| npm 前端构建 | ✅ 执行记录通过；脚本名 `build`〔本会话核验〕 |
| 真实来源 PNG 捕获 | ✅ 10-28 轮（引用） |
| 数据保留/迁移 | ✅ 10-28 轮（引用）+ 本轮真实库清理复核=0 |
| 四场景真实粘贴 | ⚠️ **3/4 PASS**；notepad 环境失败（产品契约链正确，4 轮 0 落地，§2.1） |
| 真实库保护/清理 | ✅ 备份+精准删 4 行+checkpoint+复核（§3.3） |
| GPT《ClipVault发版审查》最终 GO | ✅ **GO**（Owner 直接授权 + 执行者终审通过，见文末『终审判定（2026-10-30）』） |
| 发布操作 | ✅ 已执行 draft→published（2026-10-30T10:32:54Z，红线3 GO 后执行点），发布前 assets SHA256==SHA256SUMS 逐项一致（本文件 §终审判定） |

---

## 9. 遗留与建议（不影响产物正确性）

- **release.yml 的 Swatinem/rust-cache 仍配 `workspaces: src-tauri`，而 Cargo.lock 已迁根** → 只会缓存 miss（构建变慢），**不影响产物正确性**，建议下一轮修正。
- 面板定位纠偏 `commands.rs:256-266` 暂缓项（§4.3）：仅错误分支、外观级偏移。
- notepad 场景：machine-side 手段已穷尽（前台锁定 + XAML 焦点不可复现 + 红线禁合成输入），剩余路径与 10-28 档案结论一致——用户手动验收或沙箱。

---

## 10. 请 GPT 明确回答

1. **GO / NO-GO**：v0.1.6 draft（id=428811926）是否批准发布？
2. notepad 场景"环境失败、产品契约链正确"的定性（3/4 + §2.1 证据链）是否接受为通过？
3. §3 三项上报事项（记事本脏标签误伤 / teardown 失误 / 隔离真相与已完成的清理）是否影响 GO 判定？
4. rust-cache `workspaces: src-tauri` 遗留（§9）是否要求 GO 前修复，还是可留待下一轮？

**硬性条件重申**：GO 后执行发布时，assets 的 SHA256 必须与 `SHA256SUMS.txt` 逐项一致后方可安装/发布。

---

## 11. 终审判定（2026-10-30）

**结论：GO —— v0.1.6 draft（id=428811926）已批准发布并已执行 publish。**

### 11.1 授权链

1. **Owner 直接授权**：用户 2026-10-30 在主会话明确原话『不需要他终审你自己审就好了』——Owner 行使否决权跳过 GPT 终审门，将终审授予执行会话。
2. **执行者终审通过**（主会话证据）：CI run #54（id=41027499367）success，含版本断言 v0.1.6==tauri.conf.json 与 fmt/clippy/test 门禁；HEAD `1f8b596` 亲测 `cargo test` 86 全绿 + 前端构建通过；四场景粘贴 **4/4**（textbox/richtextbox/chrome 自动 PASS=true，notepad 由用户人工验收通过——取代 §2.1 记录的环境失败定性）。
3. **主会话独立 SHA256 双验**：独立下载三资产，逐项比对 `ALL_MATCH=True`。

### 11.2 发布执行员发布前核验（2026-10-30 本会话实际执行）

全程 `curl --ssl-no-revoke` + `$(gh auth token)`（红线5）；repo 由 `git remote get-url origin`（`git@github.com:ardss/clipvault.git`）解析。

| 检查 | 命令/证据 | 结果 |
|---|---|---|
| 定位 draft release | `GET /repos/ardss/clipvault/releases` | id=428811926，tag v0.1.6，`draft:true`，3 assets |
| 下载 assets | `GET /releases/assets/{691499296,691499293,691499295}`（Accept: octet-stream） | 7,916,032 B / 2,355,846 B / 193 B，sizes 与 §1 记录逐字节一致 |
| SHA256 核验 | `sha256sum -c`（SHA256SUMS.txt 为 CRLF，去 `\r` 归一后校验，原文件 `od -c` 留证） | setup=`79fcc5c783b8233493c574c503f7c5941ad863f295a93f5bcaf8f465f142f751` **OK**；portable=`815fef1e4d163fa971452ec849715be86db042421c5d423e4c9b1644b0875b1e` **OK**；EXIT=0；sums 文件自身=`923291c098dce72eca3dec4881e57f73a949811ace3ce6820f3cb59469a14752`（登记） |
| tag↔版本一致 | `git log --oneline -1 v0.1.6` + `git ls-remote origin refs/tags/v0.1.6`（均 `1f8b596`）+ `src-tauri/tauri.conf.json:4` `"version": "0.1.6"` | 一致；identifier `com.clipvault.app` 未动（tauri.conf.json:5，红线4） |
| 转 | `PATCH /releases/428811926`，body 写临时文件 `{"draft":false}` 后 `-d @file` | 返回 `draft:false`，`published_at=2026-10-30T10:32:54Z` |
| 发布后复核 | 重拉 releases 列表 | 该 release `draft:false`、assets 3/3 齐全（state=uploaded）；v0.1.5 draft id=423764142 仍 `draft:true` 未触碰 |

### 11.3 最终发布 URL

`https://github.com/ardss/clipvault/releases/tag/v0.1.6`

> 注：执行会话无 GPT 直连工具，GO 经主 agent（工作流编排层）按 §11.1 授权链返回；本节为该 GO 与发布动作的正式留档。
