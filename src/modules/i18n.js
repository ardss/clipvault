// UI message tables extracted from App.vue (behavior unchanged).
// Keys are shared between zh and en; i18n.spec.js asserts key completeness.
export const messages = {
  zh: {
    all:'全部', text:'文本', image:'图片', pinned:'置顶', link:'链接', file:'文件',
    search:'搜索剪贴板…', empty:'暂无内容', emptyHint:'复制任意内容（文本/图片/文件）后自动出现在这里', expired:'图片已过期',
    noMatch:'没有匹配的结果', noMatchHint:'试试更短的关键词，或按 Esc 清空搜索',
    copyHint:'点击条目粘贴 · ↑↓ 选择 · Enter 粘贴 · Del 删除', theme:'主题', lang:'语言',
    statsTitle:'统计', settingsTitle:'设置',
    statTotal:'总条目', statText:'文本', statImage:'图片', statLink:'链接', statFile:'文件',
    statPinned:'置顶', statToday:'今日新增', statPastes:'累计粘贴', statDaily:'近 7 天复制量',
    statTop:'最常粘贴 Top 5', times:'次',
    setLimit:'历史上限', setLimitHint:'超出后自动清理最旧记录（置顶除外）', setAuto:'开机自动启动', setAutoHint:'登录 Windows 后在后台静默运行', shortcut:'呼出快捷键', about:'关于', save:'保存', saved:'设置已保存', setHotkey:'呼出热键', setHotkeyHint:'如 Alt+V / Ctrl+Alt+V，保存后生效；若被占用会提示失败', setSensitive:'敏感词过滤', setSensitivePh:'token, password, 密码', setSensitiveHint:'复制的内容包含这些词时不记录（逗号分隔）', setPause:'暂停采集', setPauseHint:'开启后照常复制粘贴，但不记录任何新内容', clearAll:'清空历史', clearAllHint:'删除所有条目及其图片与附件文件，不可恢复', clearBtn:'清空全部历史', clearConfirm:'再点一次确认！', cleared:'已清空 {n} 条', pausedTag:'采集中断·已暂停',
    focusLost:'已复制到剪贴板，但无法聚焦原窗口，请手动 Ctrl+V', pasteFail:'粘贴失败，内容仍在剪贴板', dayToday:'今天', dayYesterday:'昨天', dayEarlier:'更早',
    demoPasted:'在线演示：实际使用时这里会粘贴到原窗口',
    pin:'置顶', del:'删除', clear:'清除',
  },
  en: {
    all:'All', text:'Text', image:'Image', pinned:'Pinned', link:'Links', file:'Files',
    search:'Search clipboard…', empty:'Nothing here yet', emptyHint:'Copy anything (text / image / files) and it shows up here', expired:'Expired',
    noMatch:'No matching entries', noMatchHint:'Try a shorter keyword, or press Esc to clear the search',
    copyHint:'Click to paste · ↑↓ select · Enter paste · Del delete', theme:'Theme', lang:'Lang',
    statsTitle:'Stats', settingsTitle:'Settings',
    statTotal:'Total', statText:'Text', statImage:'Images', statLink:'Links', statFile:'Files',
    statPinned:'Pinned', statToday:'Today', statPastes:'Pastes', statDaily:'Last 7 days',
    statTop:'Top 5 pasted', times:'×',
    setLimit:'History limit', setLimitHint:'Oldest unpinned items are cleaned up beyond the limit', setAuto:'Launch at startup', setAutoHint:'Runs quietly in the background after sign-in', shortcut:'Summon shortcut', about:'About', save:'Save', saved:'Settings saved', setHotkey:'Summon hotkey', setHotkeyHint:'e.g. Alt+V / Ctrl+Alt+V — applied on save; warns if taken', setSensitive:'Sensitive filter', setSensitivePh:'token, password, secret', setSensitiveHint:'Copies containing these words are never recorded (comma separated)', setPause:'Pause capture', setPauseHint:'Copy and paste as usual — nothing new is recorded while on', clearAll:'Clear history', clearAllHint:'Deletes every entry with its images and side files. Cannot be undone.', clearBtn:'Clear all history', clearConfirm:'Click again to confirm!', cleared:'Cleared {n} item(s)', pausedTag:'Capture paused',
    focusLost:'Copied to clipboard, but could not focus the previous window — press Ctrl+V manually', pasteFail:'Paste failed, content stays on clipboard', dayToday:'Today', dayYesterday:'Yesterday', dayEarlier:'Earlier',
    demoPasted:'Demo: the real app pastes this into your previous window',
    pin:'Pin', del:'Delete', clear:'Clear',
  },
}

// Pure lookup: unknown keys fall through to the key itself (same as before).
export function translate(lang, key) {
  return (messages[lang] ?? messages.zh)[key] ?? key
}

export function documentLangAttr(lang) {
  return lang === 'zh' ? 'zh-CN' : 'en'
}
