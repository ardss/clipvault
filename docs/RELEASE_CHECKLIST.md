# 发布清单（Release Checklist）

> 测试规范见 [TESTING.md](TESTING.md)。以下每一条都必须逐项核对，全部通过才允许发布。

## 1. 版本号五处一致

以下位置的版本号必须完全相同：

1. `package.json`
2. `src-tauri/tauri.conf.json`
3. `src-tauri/Cargo.toml`
4. `Cargo.lock`（仓库根，工作区锁文件；根包名为 `app`：`cargo update -p app` 或重新构建后自动同步）
5. `CHANGELOG.md`（新增对应版本条目）

## 2. 门禁 + 深度测试

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -- --ignored   # deep suite
npm run build
npx tauri build   # 仅 CI 执行——本仓库正式候选包唯一来源是 GitHub Release CI；Windows B 当前环境禁止本地跑（环境操作限制，非 Tauri 通用约束）
```

## 3. 安装包内容新鲜度

- `dist/` 目录时间戳必须**早于**安装包（bundle）时间戳 — 保证打包的是最新前端产物
- 本次修复的关键字符串必须能在 `dist/assets/` 里 `grep` 到（防止打包了旧产物）

```bash
grep -r "<关键修复字符串>" dist/assets/
```

## 4. 静默安装验证流程（逐核物理证据，不许跳步）

1. **杀掉旧进程**（taskkill），并确认进程真的退出了（tasklist 里查不到）
2. `安装包.exe /S` 静默安装
3. 核对安装后的 exe `LastWriteTime` 已经更新（PowerShell: `(Get-Item $exe).LastWriteTime`）— **不要只看注册表版本号**
4. 启动应用
5. 核对进程 `StartTime` 是刚刚启动的时间（`Get-Process` 的 `StartTime`）

> 教训：曾出现注册表版本号正确、但旧 exe 仍在运行 / 文件未被替换的情况，导致带 P0 缺陷的版本被误认为验证通过。

## 5. SHA256 记录与 tag

- 记录安装包 SHA256（`certutil -hashfile <file> SHA256`）
- 推送 tag，等待 CI 生成 Release 草稿
- 核对 CI 草稿：**本地构建与 CI 构建不可复现（字节级不同），最终以 CI 产物附带的 SHA256SUMS 为准**，发布页以它为准，不要用本地哈希替换

## 6. 已知风险点

- **并发构建会导致 cargo 锁冲突（EXIT 127）**。构建前先杀残留 cargo/rustc 进程：

```bash
taskkill //F //IM cargo.exe //T 2>/dev/null; taskkill //F //IM rustc.exe //T 2>/dev/null
```

- 始终用 `npx tauri build`，不要用裸 `cargo build --release`（会带 dev 配置，见 postmortem-2026-09-11）

## 7. 发布操作门禁（2026-10-26 误发布事故教训，GPT 批准）

> 背景：v0.1.4 候选 Draft 因 `form.requestSubmit()` 程序化提交被 GitHub 服务端按发布处理，意外公开（2026-10-27）。事后取证确认已发布 Release **不可转回 Draft**（见下「另」注），最终处置为正文顶部警告加固并保持公开——并未回滚。以下 4 条为发布操作硬门禁：

1. **禁止程序化提交 Release 表单**——BrowserOS/自动化对 GitHub Release 编辑表单禁用 `requestSubmit()` 及任何未经验证的程序化提交路径
2. **Draft 状态变更只走认证 API**——如 `PATCH /repos/{o}/{r}/releases/{id} -f draft=false`（草稿转正），不用 Web 表单；注意反向 PATCH `draft=true` 只对真 Draft 有效，对**已发布** Release 会被 GitHub 静默忽略（HTTP 200 但状态不变，2026-10-27 诊断 runs 40416206148 / 40419151569 取证）
3. **写后读取验证**——每次状态变更后重新 GET/回读确认，不以 HTTP 成功、页面跳转或按钮状态代替
4. **draft=false（对外发布）须单独最终 GO 授权**——不得由定时巡检或通用维护 Agent 自主触发

另：已发布 Release 无 Web UI「转回草稿」入口，认证 API PATCH `draft=true` 同样被平台静默忽略——**已发布版本没有「转回 Draft」回滚路径**。实际处置二选一：正文警告加固（v0.1.4 先例）或删除 Release/tag/资产重建（须 GPT/用户明确授权）；不得未经授权删对象绕过，保留警告并升级裁决。
