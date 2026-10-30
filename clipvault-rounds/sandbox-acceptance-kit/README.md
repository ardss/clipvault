# ClipVault v0.1.5 隔离验收套件（Windows Sandbox）

> 对应 GPT《ClipVault发版审查》2026-10-27 裁决：隔离 VM 回归 **CONDITIONAL APPROVE**。
> 四条件 → 证据映射见下。执行产出 `vm-share\out\`（截图/result.json/clips.db/cv.log）即终审证据包。

## 前置（一次性，需要管理员 + 重启）
```powershell
Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -NoRestart
```
重启后 `C:\Windows\System32\wsb.exe` 存在即为就绪。（2026-10-28 17:49 UAC 触发被取消，功能仍未启用）

## 执行（重启后任意轮）
```powershell
pwsh -NoProfile -File K:\Project\1-Active-Projects\clipvault\clipvault-rounds\sandbox-acceptance-kit\run-acceptance.ps1
```
自动完成：功能预检 → 候选包缺失时经孤儿分支从 v0.1.5 Draft 拉取（token 认证 API）→ 宿主侧 SHA256 终核 → 启动 Sandbox。

## Sandbox 内自动执行（vm-acceptance.ps1，全程合成输入仅 VM 内）
| 步骤 | 内容 | 对应 GPT 条件 |
|---|---|---|
| S1 | 安装包 SHA256 对 SHA256SUMS.txt 逐字核验 | ✅ 安装器哈希验证 |
| S2/S3 | /S 静默安装 + app.exe mtime/StartTime 物理核验（RELEASE_CHECKLIST §4） | 发布门禁 §4 |
| S4 | 记事本真实粘贴（Alt+V→过滤→Enter→剪贴板回读断言） | ✅ 场景1 |
| S5 | Edge 本地页 textarea 真实粘贴+回读断言 | ✅ 场景2 |
| S6 | Explorer 地址栏原生多控件粘贴 + COM 导航断言 | ✅ 场景3 |
| S7 | 真实来源图片捕获（mspaint Ctrl+A/C；无 Paint 时回退 WPF ImageCopyTool 真实 GUI 应用），断言图片库增长 | ✅ 真实来源图片捕获 |
| S8 | 证据打包（clips.db/cv.log/截图/result.json → out\） | 证据链 |
| .wsb | ClipboardRedirection=Disable / Networking=Disable / vGPU=Disable | ✅ 剪贴板隔离+合成输入不出 VM |

## 闭环链
Sandbox 跑完 → `out\` 证据包 → 投 GPT 独立终审（原《ClipVault发版审查》会话）→ 最终 GO → 认证 API 发布 v0.1.5（§7 门禁）→ 联动 Windows A/B 双机安装（数据不丢+运行验收，四格 PASS 才通知用户）。
