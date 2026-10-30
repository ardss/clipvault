# 2026-10-27 v0.1.4 误发布回滚取证（incident evidence）

## 事实时间线
- 2026-10-26 08:57:19 +08:00：v0.1.4（release id **422588922**，tag v0.1.4 → f26027e）被程序化表单提交意外发布；title 已带 `v0.1.4 [SUPERSEDED — DO NOT PUBLISH]` 警告。
- 2026-10-26 16:57：代理 GitHub TLS 断线，回滚 PATCH 卡网络层；守望者 v2 挂至次日 06:30。
- 2026-10-27 06:30：守望者到期，gh 未登录，回滚未发生。
- 2026-10-27 06:42：TLS 恢复（api.github.com 200）；gh CLI 仍未登录；用户浏览器 GitHub 会话已失效（device 流授权页重定向 login 页）→ 设备流通道死。
- 2026-10-27 06:46：按「穷尽技术路径换通道」改用仓库 GITHUB_TOKEN：临时分支 `ops/v014-rollback` workflow（认证 API，符合 §7.2）执行 `PATCH /releases/422588922 -f draft=true`。run 40415740119 **成功**，但发布状态未变。
- 2026-10-27 06:56：诊断 workflow（`ops/v014-diag` / `ops/v014-diag2`）取证，结果经孤儿分支 `ops/diag-result` result.md 匿名回读。

## 平台行为实锤（diag2 result.md 原文回读）
```
## 1. GET before
{"draft":false,"id":422588922,"published_at":"2026-10-26T00:57:19Z","tag_name":"v0.1.4"}
## 2. PATCH -f draft=true : response draft field
patch response -> "draft":false        ← HTTP 200，字段被静默忽略
## 3. GET after
{"draft":false,"published_at":"2026-10-26T00:57:19Z"}
## 4. name-edit probe (PATCH name=<same title>)
probe response -> {"draft":false,"name":"v0.1.4 [SUPERSEDED — DO NOT PUBLISH]"}
                                       ← 编辑权限正常（name/body 可改）
## 5. anon visibility
anon GET by tag -> HTTP 200            ← 公开态持续
```
- 结论：**已发布 release 无法经 API 转回 draft（平台限制，非权限问题）**；10-26 日志「PATCH 唯一路径」假设不成立。§7「PATCH 被拒不删 Release 绕过」生效 → 未删任何对象，升级裁决。

## 暴露期下载计数（06:47 匿名 API）
- ClipVault_0.1.4_x64-setup.exe：1 次
- ClipVault_0.1.4-portable.exe：0 次
- SHA256SUMS.txt：4 次
- 合计 5 次下载；Latest 始终保持 v0.1.3（08:57 事故后匿名 /releases/latest 全程核验未变）。

## Actions run 留痕（公共可见，分支已删）
- run 40415740119 `ops-rollback-v014`：success（PATCH 已发、无状态变化）
- run 40416206148 `ops-diag-v014`：success（同结论）
- diag2 run（分支 ops/v014-diag2）：success（结果落 result.md，见上）
- 临时分支 ops/v014-rollback、ops/v014-diag、ops/v014-diag2、ops/diag-result 用毕即删，不进 main。

## 待 GPT 裁决选项
- A（推荐）：保留公开态 + 用已验证可用的编辑权限加固 body（大字警告：误发布、含 P1 缺陷、请用 v0.1.3 或待发布 v0.1.5）；历史暴露事实保留符合关闭条件 #5。
- B：删 release 重建 draft（需 GPT 明确授权解除 §7 不删禁令；资产需重跑构建，动作重）。
- C：维持现状（title 已警告），v0.1.5 发布说明中记载事故。
