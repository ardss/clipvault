# 发布清单（Release Checklist）

> 测试规范见 [TESTING.md](TESTING.md)。以下每一条都必须逐项核对，全部通过才允许发布。

## 1. 版本号五处一致

以下位置的版本号必须完全相同：

1. `package.json`
2. `src-tauri/tauri.conf.json`
3. `src-tauri/Cargo.toml`
4. `src-tauri/Cargo.lock`（`cargo update -p clipvault` 或重新构建后自动同步）
5. `CHANGELOG.md`（新增对应版本条目）

## 2. 门禁 + 深度测试

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -- --ignored   # deep suite
npm run build
npx tauri build
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
