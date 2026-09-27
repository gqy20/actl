# 15 · 系统 UI 区域 UIA 可达性普查(2026-09-27)

方法:唤出区域 → 窗口差集 → `snapshot --hwnd` 深读 → 关闭。Win 组合键经
actl 原生 `press win+x`(前台窗锚定——任务栏不可聚焦,--app 需锚当前前台,
shell 级注入的如实语义)。

| 区域 | 判定 | 依据 |
|---|---|---|
| **快速设置**(Win+A) | ✅ 全可达 | ControlCenterRegion → RootGridView;WLAN/蓝牙/勿扰/媒体控件等按钮**实名可点** |
| **通知中心**(Win+N) | ✅ 全可达 | 通知列表/请勿打扰/全部清除/通知来源(含"来自 ZCode 的通知")实名 |
| **桌面图标** | ✅ 全可达 | Program Manager → ListItem(回收站/study/…) |
| **开始菜单**(单键 win) | ✅ 全可达 | 窗口预创建(title"开始"/CoreWindow),差集法曾误判;搜索框 Edit+AppsList+钉住应用实名(Word/Edge…) |
| 任务栏/托盘 | ❌ UIA 退化 | 前轮终审(全树 3 空 Pane;路线=截图+坐标点击/Shell API,M3) |

## 更正入档

"键盘引擎不含 Win 键是安全边界"为**误判**(两轮口误):keys.rs 的 MODIFIERS
自始含 `win`(→VK_LWIN),清场 11 键含双 Win 变体——**引擎一直支持,无需改动**。
行业对照:禁 Win 组合是 Codex 的产品策略(docs/12),非本项目的边界。

## 结论

系统 shell 层三分:"**flyout 系全可达**(快速设置/通知中心——系统控制原语
路线直接可用:click name:蓝牙 等)","**经典层可达**(桌面)","**任务栏重写层
退化**(托盘,走视觉路线)"。pack 可直接补系统控制路线(勿扰开关/WiFi 切换)。
