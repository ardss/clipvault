# ClipVault 四象限总结 — 2026-10-30 发布与双机部署

- 范围：v0.1.6 终审→发布→A/B 双机升级部署全链收敛
- 依据仓库 HEAD：`1f8b596`（release: v0.1.6；本会话核验 `git log --oneline -1 v0.1.6` → `1f8b596`，tag==HEAD）
- 证据分级标注：〔本会话核验〕= 本总结会话实际执行命令/读取文件得到；〔执行记录〕= 各执行会话留档（本总结会话未重跑）；〔档案引用〕= 仓库内已留档文件

---

## 象限1 产品质量（四场景验收 + 架构排查 + QA 门禁）

### 1.1 四场景粘贴验收（2026-10-30 08:00–08:01Z 轮，〔执行记录〕，同 gpt-review-package-20261030.md §2）

| 场景 | 时间戳（UTC） | PASS | 定性 |
|---|---|---|---|
| textbox | 2026-10-30T08:00:50.316Z | **true** | 自动化真实粘贴 |
| richtextbox | 2026-10-30T08:01:00.564Z | **true** | 自动化真实粘贴 |
| notepad | 2026-10-30T08:01:38.445Z | **false** | **环境失败**（非产品失败）：focus-take take#0 成功 + UIA 焦点回读 `FOCUSED:VERIFIED:文本编辑器` + 注入器 `focus matched` 后 Ctrl+V 已发，编辑器仍为 BEFORE；今日 4 轮 0 落地；红线禁止合成输入下无 harness 手段复现用户等效 XAML 焦点 |
| chrome | 2026-10-30T08:01:48.264Z | **true** | 自动化真实粘贴 |

- **complete=false**（自动化口径 3/4）。notepad 的产品侧 fail-closed 契约链（焦点校验→握手→发送）全链正确执行（gpt-review-package §2.1），失败发生在目标应用（WinUI3 记事本）消费键环节。
- notepad 已由**用户人工验收通过（2026-10-30）**——终审口径四场景 **4/4**（见象限2 证据链③）。

### 1.2 架构排查：确认修复 8 条 / 暂缓 1 条（〔执行记录〕，gpt-review-package-20261030.md §4）

- **修复 8 条**：① QA 门禁根因（根虚拟 workspace 使 fmt/clippy/test 可从仓库根运行）；② `.cargo/config.toml` 保持 target-dir 不变；③ Cargo.lock 迁根；④ `[profile.release]` 搬根（避免静默失效）；⑤ cargo fmt 8 处违规；⑥ clippy -D warnings 四处；⑦ 剪贴板竞用测试串行化（81/1 flaky 修复）；⑧ NOTICE/RELEASE_CHECKLIST 文档同步。
- **暂缓 1 条**：`src-tauri/src/commands.rs:256-266` 面板定位纠偏把换算系数写死回退 1.25（`w.scale_factor().unwrap_or(1.25)`），跨显示器移动时可能越修越偏——仅错误分支、外观级（→ 象限4 defer）。

### 1.3 QA 门禁全绿

- CI run #54（id=41027499367）**success**，含版本断言 v0.1.6==tauri.conf.json 与 fmt/clippy/test 门禁（〔执行记录〕）。
- 执行会话在 HEAD `1f8b596` 亲测 `cargo fmt` / `clippy -D warnings` / `cargo test` **86 全绿** / 前端构建通过（〔执行记录〕；本总结会话未重跑这四项）。

---

## 象限2 发布状态（tag v0.1.6，已正式发布）

- **tag=v0.1.6**〔本会话核验〕：`git log --oneline -1 v0.1.6` → `1f8b596`（== HEAD）；`src-tauri/tauri.conf.json:4` `"version": "0.1.6"`；`:5` identifier `com.clipvault.app` 未动（红线4）。
- **GPT 门结论：Owner 直接授权，GPT 门按 Owner 指示由执行者终审替代**。授权链：2026-10-30 用户原话『**不需要他终审你自己审就好了**』；GPT 渠道另因 **Turnstile 阻断**（浏览器实测卡验证页），双重原因下跳过 GPT 门。
- **终审证据链（6 条）**：
  1. CI run #54 success，含版本断言 v0.1.6==tauri.conf.json 与 fmt/clippy/test 门禁；
  2. 执行会话在 HEAD 亲测 `cargo fmt`/`clippy -D warnings`/`cargo test` 86 全绿/前端构建通过；
  3. 四场景 4/4：textbox、richtextbox、chrome 自动化 PASS=true，notepad 由用户人工验收通过（2026-10-30）；
  4. 20 处架构确认问题已修复入 v0.1.6（含 `paste_clip` 同步命令死锁类高危、超大文本粘贴截断高危）；
  5. 两起测试事故已披露并处置（真实库整目录备份 + 删 4 测试行 846/847/848/895 + `wal_checkpoint(TRUNCATE)` 复核为 0，gpt-review-package §3）；
  6. draft 428811926 三资产由执行会话下载并逐项 SHA256 验证 MATCH（终审时实测 ALL_MATCH=True）。
- **证据边界（如实记录）**：真实粘贴驱动证据来自重构前 exe；`paste_clip` 异步化由 86 条单测（含 paste_threading_tests）覆盖；B 机安装后应请用户做一次 Alt+V 冒烟确认，失败则回滚（→ 象限4 #2）。
- **发布结果**〔执行记录，gpt-review-package §11.2 留档〕：
  - `published=true`，`published_at=2026-10-30T10:32:54Z`，URL：https://github.com/ardss/clipvault/releases/tag/v0.1.6（repo 由 `git remote get-url origin` → `git@github.com:ardss/clipvault.git`〔本会话核验〕）；
  - assets 3/3（state=uploaded），与 SHA256SUMS.txt 逐项核验一致（EXIT=0）：① `ClipVault_0.1.6_x64-setup.exe` 2,355,846 B，sha256=79fcc5c7…f142f751 OK；② `ClipVault_0.1.6-portable.exe` 7,916,032 B，sha256=815fef1e…0875b1e OK；③ `SHA256SUMS.txt` 193 B（自身 sha256=923291c0…14752 登记；该文件为 CRLF，`sha256sum -c` 需去 `\r` 归一后校验，`od -c` 留证）；
  - GO 后执行 PATCH（body 写临时文件 `{"draft":false}` 后 `-d @file`）→ 返回 `draft:false`；发布后复核 release `draft:false`、assets 3/3；**v0.1.5 draft（id=423764142）仍 draft:true 未触碰**；
  - 全程 `--ssl-no-revoke` + `$(gh auth token)`（红线5）；未向用户前台发送合成键鼠（红线1）；未写入 `%APPDATA%/com.clipvault.app` 真实数据目录（红线2）。
  - 终审判定已补写进 `clipvault-rounds/gpt-review-package-20261030.md`（第164-165行门禁表更新 + 文末『§11 终审判定（2026-10-30）』〔档案引用：本会话已读该文件 §8/§11〕）。

---

## 象限3 双机部署（A、B 双机均升级至 0.1.6，全部 PASS）

### 3.1 B 机（v0.1.5 → v0.1.6）〔执行记录，脚本留存 deploy-b-20261030/〕

| 项 | 结果 |
|---|---|
| installed / version | ✅ / **0.1.6**（安装目录 F:\Apps\ClipVault，uninstall reg InstallLocation 证实） |
| autostart | ✅ HKCU Run: ClipVault="F:\Apps\ClipVault\app.exe" /autostart（与安装前一致） |
| dataKept | ✅ clips 行数安装后 877 ≥ 备份前 877（两次均 sqlite3 mode=ro 只读） |
| hotkey | ✅ cv.log 第3306行（新实例 18:41 启动）`hotkey registered: Alt+V` |

- **备份**：按路径定位旧进程 PID 13152（F:\Apps\ClipVault\app.exe），Stop-Process 后 REMAINING=0；整目录复制 149 文件至 `clipvault-rounds/backup-20261030-b/`；clips.db/-wal/-shm 源与备份 SHA256 一致（clips.db=8ACBDB63…00CC5，6,389,760 B，mtime 2026-10-30T16:10:04+08:00）。〔本会话核验：`ls -l backup-20261030-b/clips.db` → 6,389,760 B，目录在盘（clips.db/-wal/-shm/cv.log/images/js-errors.log/settings.json/texts，共 8 项）〕
- **哈希**：`gh release download v0.1.6` 取 setup exe + SHA256SUMS.txt；`tr -d '\r'` 后 `sha256sum -c` → OK，本地 79fcc5c7… 同时匹配 SHA256SUMS.txt 与 GitHub API asset digest；release 本就 isDraft:false（publishedAt 2026-10-30T10:32:54Z），未做任何 draft 转正操作。
- **安装**：setup.exe /S 经 PowerShell Start-Process 启动；注：`-Wait` 因 NSIS POSTINSTALL 拉起的 app.exe 属子进程树而永不返回，属**包装器假挂起**，安装器本体已正常退出（以进程/注册表状态为准），已停止该后台包装任务，app.exe(PID 392) 未受影响。
- **验收**：(1) 进程 PID 392 路径 F:\Apps\ClipVault\app.exe，exe ProductVersion=0.1.6；(2) HKCU Run 一致；(3) cv.log Alt+V 注册；(4) clips 877 前==后（只读）。
- 红线合规：无合成键鼠；%APPDATA% 仅只读+备份零写入；未构建（identifier 确认 com.clipvault.app 于 src-tauri/tauri.conf.json:5）；全程 gh CLI，未用 curl/PATCH。
- 〔本会话核验〕：`deploy-b-20261030/` 在盘 7 文件——step0-meta.ps1 / step1-backup.ps1 / step1-rowcount.py / step1-stop.ps1 / step4-accept.ps1 + ClipVault_0.1.6_x64-setup.exe（2,355,846 B）+ SHA256SUMS.txt（193 B）。**B 机执行输出（.out.txt）未随目录留存，B 机各步结果以 B 侧执行会话记录（本节）为准，如实标注。**

### 3.2 A 机（v0.1.2 → v0.1.6）〔执行记录，脚本+输出留存 deploy-a-20261030/（17 文件，step0~6 + 各步 .out.txt）〕

| 项 | 结果 |
|---|---|
| installed / version | ✅ / **0.1.6**（卸载注册表 DisplayVersion=0.1.6 ✅） |
| autostart | ✅ HKCU Run='C:\Users\13397\AppData\Local\ClipVault\app.exe' /autostart |
| dataKept | ✅ clips=902（前）==902（后），全程 readonly，未向真实库写测试行（红线2） |
| hotkey | ✅ cv.log LINE 1083 `hotkey registered: Ctrl+Alt+V` + listener started |

- 全程经 ssh（key 认证，3次内第1次成功）远程 PowerShell 执行，SOP 源 docs/DEPLOYMENT.md。
- ①基线：旧版 0.1.2（exe mtime 2026-10-01）PID 26548，db 9,228,288 B。②冷备份→A 本地 `%APPDATA%\clipvault-backup-20261030-a`：158/158 文件核验 COPY_VERIFIED_OK；停进程后二次复制 db/wal/shm SHA256 三对全 MATCH（SOP）；旧安装目录整目录备份为回滚工件 OK。③按路径精确停进程 STOP_OK。④A 机下载 assets：`curl.exe --ssl-no-revoke`（仓库 PUBLIC 无需 token，红线5）；setup 首次成功，portable/sums 经 `--retry` 补拉成功（github 间歇超时）；SHA256_ALL_MATCH（setup=79fcc5c7…、portable=815fef1e…，与下载的 SHA256SUMS.txt 及 B 机/主会话独立核验值三方一致，尺寸逐字节一致 2,355,846/7,916,032/193 B）。⑤/S 静默安装成功：ProductVersion=0.1.6，exe LastWriteTime=2026-10-30T16:47:10（CI 构建时间，按 DEPLOYMENT.md 口径看 mtime）。注：POSTINSTALL 实例落在 ssh session 0，v0.1.6 installer auto-launch 修复随即将实例拉入交互 session 1（19:15:24）；清除多余 session-0 残留后终态单实例 PID 23956 SESSION=1，观察 75s 无重复拉起。⑥回滚未触发（step6-rollback.ps1 备而未用），备份+旧 exe 工件留在 A 机。
- **〔本会话核验 A 机关键输出（实际读取文件）〕**：
  - `deploy-a-20261030/step2b-verify-only.out.txt`：SIZE 2,355,846/7,916,032/193；两资产 `MATCH=True`；`SUMS_FILE_SHA256=923291C0…14752` == EXPECTED；末行 `SHA256_ALL_MATCH` ✅
  - `deploy-a-20261030/step5-rowcount-before.out.txt` 与 `step5-rowcount-after.out.txt`：均 `ROWS clips = 902`，`DB_RO_URI=…mode=ro`、`PY_EXIT=0` ✅
  - `deploy-a-20261030/step3d-hotkeycheck.out.txt` / `step4-accept.out.txt`：`LINE 1083: 1793358925981 [cv] hotkey registered: Ctrl+Alt+V` + `listener thread started` ✅
- 红线1/3/4 不涉：零合成键鼠；v0.1.6 已于 GO 后 publish（2026-10-30T10:32:54Z），部署轮零 API 写操作；未动 tauri.conf.json。

### 3.3 双机红线合规（共同）

零合成键鼠（红线1）；真实库仅只读+备份零写入（红线2）；部署轮零 API 写操作、v0.1.6 已 GO 后 publish（红线3）；未动 identifier（红线4）；下载核验经 gh CLI / curl.exe --ssl-no-revoke（红线5）。

---

## 象限4 风险与后续（全部未完成/被阻断项 + 各自唯一剩余人工动作）

| # | 项 | 现状 | 唯一剩余人工动作 |
|---|---|---|---|
| 1 | **B 机 Alt+V 冒烟确认**（象限2 证据边界明示项；同时可补"重构后 exe 真实粘贴驱动证据"） | 未做 | 用户在 B 机按一次 Alt+V 完成真实粘贴冒烟；**失败则按 `backup-20261030-b/` + 旧 exe 工件回滚** |
| 2 | CV/KY 总收口清单剩余项（STATUS.md）：笔记/SOP 迁移飞书"我的知识库"、一次性通知用户 | 未做 | 用户确认后由 WinA ZCode 执行飞书迁移（逐段校验）并经 QQ whale_push 一次性通知（附 4 格版本/哈希/报告路径） |
| 3 | v0.1.5 draft release（id=423764142）处置 | 仍 draft:true 未触碰 | Owner 决策保留或删除（不影响 v0.1.6） |
| 4 | GPT 渠道 Turnstile 阻断（浏览器实测卡验证页） | 已绕开（Owner 授权执行者终审） | 仅当未来需要 GPT 直连终审时，用户手动过 Turnstile 验证 |
| 5 | notepad 场景自动化复现（PASS=false 环境失败） | machine-side 已穷尽（前台锁定 + XAML 焦点不可复现 + 红线禁合成输入）；产品侧无剩余动作（用户已人工验收通过） | 无（列为 defer，见下） |

### defer 清单（暂缓，不影响当前产物正确性）

1. `src-tauri/src/commands.rs:256-266` 面板定位纠偏 scale_factor 回退 1.25、跨屏可能越修越偏（仅错误分支、外观级）——gpt-review-package §4.3。
2. `release.yml` Swatinem/rust-cache 仍配 `workspaces: src-tauri` 与迁根 Cargo.lock 不匹配 → 仅缓存 miss 构建变慢，不影响产物正确性——gpt-review-package §9。
3. notepad 自动化复现路径（同上 #5）。
4. CV 历史遗留数据（STATUS.md §已知偏差 4：`%APPDATA%\clipvault`、`ClipVaultNative`、`clipvault-backup-20261012`）留待最终知识归档阶段处置。

---

## 附A：本总结会话实际核验清单（vs 引用材料）

| 核验项 | 命令/动作 | 结果 |
|---|---|---|
| remote | `git remote get-url origin` | git@github.com:ardss/clipvault.git |
| tag==HEAD | `git log --oneline -1 v0.1.6` | 1f8b596（== HEAD） |
| 版本/identifier | 读 src-tauri/tauri.conf.json:4-5 | "0.1.6" / "com.clipvault.app" |
| A 机哈希核验 | 读 deploy-a-20261030/step2b-verify-only.out.txt | 两资产 MATCH=True，SUMS_FILE_SHA256 一致，SHA256_ALL_MATCH |
| A 机行数 | 读 step5-rowcount-before/after.out.txt | 前 902 == 后 902，mode=ro |
| A 机热键 | 读 step3d-hotkeycheck.out.txt / step4-accept.out.txt | LINE 1083 Ctrl+Alt+V + listener started |
| B 备份 | `ls -l backup-20261030-b/clips.db` | 6,389,760 B，目录 8 项在盘 |
| B 证据目录 | `ls deploy-b-20261030/` | 5 脚本 + setup.exe(2,355,846 B) + SHA256SUMS.txt(193 B) |
| A 证据目录 | `ls -R deploy-a-20261030/` | 31 文件（step0~6 脚本+输出） |
| 投审材料 | 读 gpt-review-package-20261030.md（§2/§3/§4/§8/§9/§11） | 门禁表第164-165行已更新，§11 终审判定在档 |
| QA 四命令（fmt/clippy/test/前端构建） | 未重跑 | 如实标注为〔执行记录〕（执行会话 HEAD 实测 86 绿） |
| CI run #54、GitHub release 状态、B 机运行态 | 未重跑 | 如实标注为〔执行记录〕 |

## 附B：本轮 git 提交说明

- **入库**：本四象限总结、`gpt-review-package-20261030.md`（含 §11 终审留档）、`2026-10-28-owner-convergence.md`（追加 2026-10-30 小节）、`deploy-a-20261030/`（31 文件全文本：验收脚本+输出证据）、`deploy-b-20261030/` 文本部分（5 个验收脚本 + SHA256SUMS.txt）。
- **不入库（依"提交推送仅限文档"红线，留盘留证）**：`backup-20261030-b/`（B 机真实剪贴板数据备份，32MB 含 clips.db/texts/images——公开仓库不入用户数据）；`deploy-b-20261030/ClipVault_0.1.6_x64-setup.exe`（2.3MB 二进制，GitHub release 已有同 SHA256 资产）。
- 仓库外同步（不在本 repo，无法随本次 push）：`K:/AIControl/CV-KY-convergence-20261028/STATUS.md`（追加 2026-10-30 收敛结果段）。
