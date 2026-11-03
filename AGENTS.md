# AGENTS — 发布治理铁律（先读这个）

**发布/交付一律遵守权威契约 `K:\AIControl\Release-Governance\DELIVERY-CONTRACT.md`**（三项目通用，2026-11-01 P0 生效）。要点：

- 状态机：CODE_PUSHED→CI_GREEN→DRAFT_READY→INSTALL_REQUESTED→MACHINE_INSTALL_PROVEN[逐台]→REAL_QA_PASSED→USER_GO→PUBLISHED。证据不足=未完成，禁止宣称已安装/已验收。
- **对外 Publish 必须用户单独 GO**；「推送/装机不用问」的授权不覆盖 Publish。
- 安装验证：现场采样机器 hostname（禁按 IP 猜）、运行 exe 绝对路径+PID、磁盘版本、**应用内运行时自报版本**（唯一合格证据）、注册表、用户数据保留。
- SSH：BatchMode=yes + NumberOfPasswordPrompts=0，密码提示立即失败。
- 用户报「还是旧版」→ 立即 FAILED 并撤销成功声明。
- 禁改仓库 visibility、禁删正式 Release。
