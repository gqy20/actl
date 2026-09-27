# 第一批深测：网易云、Zotero、VS Code、腾讯会议与夸克

后续已补测腾讯会议时间/时长点击，以及夸克搜索、筛选、详情、预览和目录导航，见 [视觉深测](28-meeting-quark-visual-deep.md)。本篇保留初轮记录。

## 环境与顺序

2026-09-28，Windows 10.0.26200.0 中文交互桌面，项目 validation release 二进制。
按网易云音乐 → Zotero → VS Code → 腾讯会议 → 夸克的顺序操作；微信跳过。
这一轮只比较同环境中的耗时。每个应用的覆盖范围单独记录，未测试的功能不算通过。

探索调用保存在 `target/batch1-deep-20260928/`，包含 JSON envelope、stderr、逐调用墙钟耗时。
独立脚本另建唯一运行目录。截图与私人账号页面只留本地，不提交仓库。
应用档案是给 agent 消费的分步路线，不能把一次观察的 HWND/ref/坐标作为持久定位。

## 网易云音乐 3.1.41.205529

本轮恢复原先最小化的窗口后，UIA 出现完整控件树，与初轮仅 6 节点不同。
尚不能证明是窗口还原、首次访问还是可访问性延迟构建触发，应先复查当前树再选后端。

已验证：

- 唯一搜索 Edit 输入中文查询并 Enter，结果标题和歌曲/歌手文本出现。
- 搜索 Edit 的 name 是变化的推荐词，不等于输入值；不能将 name 当查询读回。
- 底栏 `id:btn_pc_minibar_play` 可 UIA Invoke；其 MenuItem 名称在 play/pause 间切换。
- `Ctrl+P` 可切换同一播放状态。
- 播放列表打开后出现 Table；name:playlist 同时匹配 Image 与 MenuItem，应 find 限定角色。

`scenarios/cloudmusic-playback.ps1 -Hwnd <当前网易云 HWND>`：

1. 核对进程与窗口类，读取播放控件基线。
2. 按 UIA、键盘、键盘、UIA 顺序分别切换并恢复，共 8 次动作，每次读回状态。
3. 保存 action_ms（动作调用墙钟）和 verified_ms（动作至状态核验）。
4. 失败时不盲切状态，保留报告供检查；停止状态退出 77，不清除停止标记。

报告 `target/cloudmusic-playback-d2b9cbc88ab44c7b8b8c2a8ba1166be2/result.json`，8 次通过，
初始暂停、结束暂停；收起队列并恢复原先最小化。当前歌曲未切换，但播放进度有推进，搜索历史有测试查询。

| 路线 | 样本数 | 动作中位数 | 最小–最大 |
|---|---:|---:|---:|
| UIA Invoke | 4 | 1503.5 ms | 1428–1691 ms |
| Ctrl+P | 4 | 1587.5 ms | 1419–1627 ms |

样本很少且包含 CLI 启动、护栏等开销，只支持“本轮未观察到键盘明显快于 UIA”。
没有物理鼠标对照，不推导“键盘比点击慢”的通用结论。状态检查证明应用报告的播放状态，不验证扬声器输出。
切歌、进度跳转、音量和修改歌单本轮未测。

## Zotero 10.0.3

独立 profile/data、合成 RIS、关闭同步；不操作用户文献库。
先复验原 10 项导入/搜索检查，随后增加出版物编辑和 CSL JSON 导出。

实测：选中 Alpha 条目→定位出版物 Edit（精确过滤名称，排除“我的出版物”）→粘贴
`ACTL Journal 2026`→Tab 提交→条目右键菜单导出→CSL JSON→系统保存对话框→磁盘解析。
验证 JSON 仅有一个条目，标题、作者及 container-title 均正确。

发现与处理：

- 本轮一次 `name:文件` 同时命中 MenuItem 和 Menu，失败报告
  `target/zotero-search-0c0c9da09b344b9b9fdd248d2faa46e0/` 保留；按实际 UIA ID 改为 `id:fileMenu`。
- 动态树变化后旧 ref 的 get 返回了别的字段，进一步证实 ref 不能跨状态沿用；最终以导出文件核验。
- 系统保存窗口为独立进程，不能只按 Zotero PID 找；需核对新 HWND、class、owner_title。
- 数字 ID 1001 与地址栏重复，ID 1 与文件行/分组重复；文件名通过 find 限定 Edit，保存用 `name:保存(S)`。

扩展后的 `scenarios/zotero-search.ps1` 完整运行 11 项通过，closed_verified：
`target/zotero-search-e557ae9522d04d9fab0b7bd3ec16cb17/result.json`。
导出 JSON、数据库和哈希留在本地运行目录。附件阅读、标签编辑、分类移动未覆盖。

## VS Code 1.139.0：本轮新增用例

独立 user-data-dir、禁用扩展、仅编辑 target 下的合成文件。
在原有输入/撤销/重做/保存/重开基础上，扩展行号导航、显式替换面板，以及两个测试文件之间快速打开。

首次试跑 `target/vscode-edit-e44cee8157704b04acde99ee48317842/` 在行号插入检查失败：
Ctrl+G 返回 ok，但没有出现行号输入框，后续数字和回车进入正文。另起检查窗口也复现未出现输入框。
F1 命令面板列出了“转到行/列..., Control+G”，通过面板执行后可以出现输入框并跳到指定行。
当前只能确认快捷键路径未生效，尚未定位拦截或焦点原因；核心不做推测性修改。

脚本改为 F1→命令面板检索→核对 ListItem→UIA Invoke，再定位专用 Edit 输入行号/路径。
每个输入面先 find 限定 Edit；List 和 Edit 可能同名，避免裸 name 命中错误。
Ctrl+F 已观察到查找 Edit，但是否正确回到编辑选区仍需磁盘全文断言。

第二次完整运行 `target/vscode-edit-4a9ab1275a19448faeeb3b149245afef/` 中行号插入通过，
但 Esc 后替换选区未落盘，文件仍保留原数字；失败报告没有改写。
另起窗口验证 Ctrl+H 可打开两个命名 Edit，用 set-value 填查找/替换，点击“全部替换”及
“关闭 (Escape)”后保存，磁盘全文变为期望值。脚本因此采用显式替换面板，不再假设 Esc 后焦点位置。

最终完整运行 15 项通过，closed_verified：
`target/vscode-edit-9f644018c0b247bb9749d28ee45f4828/result.json`。
新增四项分别验证第二行定位插入、数字替换落盘、打开第二个文件及返回原文件。
命令面板与输入框名称按本轮中文界面记录，其他语言未验证。

## 腾讯会议 3.46.1.409：日期输入

沿用 docs/26 的预定草稿，不提交会议。Qt 日期框直接 set-value 返回成功但不改变日期。
本轮验证以下路径：type 开始日期 --paste 取得焦点并准备剪贴板，然后 Ctrl+A、paste、Tab，
最后 verify --value 精确读回。分别验证次日及跨月日期。

`scenarios/tencent-meeting-draft.ps1` 新增可选 SetDate，输入格式 yyyy/M/d；默认仍只核对日期。
新建表单上从 2026/9/28 改为 2026/10/2，主题与日期均通过，submitted=false：
`target/meeting-draft-ae883df9de07447eb21f6e28a18a33fa/result.json`。测试草稿已优雅关闭。

时间框为 ComboBox，弹出独立 QComboBoxPrivateContainer，未找到可用选项节点。
一次输入被前台护栏拒绝，失败报告为 `target/meeting-draft-489ad93d9c1841b38c2fc1a1780671e5/`。
关闭测试下拉框及旧草稿、重新从首页打开草稿后，日期脚本复验通过。
未绕过前台检查，未证明时间选择路径有效；时间、时长等仍需要独立验收。

## 夸克 7.2.2.1003：启动到网盘

启动前未发现夸克进程；从本机安装目录启动正常配置，显示新标签页。
已验证用户要求的完整入口：**启动夸克 → 新标签页左侧网盘入口 → 网盘首页文件列表**。
使用已有登录态，成果窗口保留在网盘首页。进入后看到了应用原有的自动备份任务在运行，未修改备份设置。

关键适配结论：

- UIA 裸 name:夸克网盘 同时命中文本和 Button；限定角色可定位浏览器工具栏按钮。
- 该 Button Invoke 返回成功，但页面仍是新标签页，不能作为已进入网盘的证据。
- 左侧可见“网盘”入口没有可调用 ref。本轮按 screenshot → hover → screenshot → 显式 physical 点击走通。
  坐标必须从当次截图和窗口边界重新确定，pack 不保存本轮固定坐标。
- 网盘首页可读取 Document、搜索 Edit、上传文件 Button、Table、文件名和文件行。
  上传文件也有同名 Text/文件行，应 find 限定 Button；裸 verify --exists 会 AMBIGUOUS。
- 搜索框可输入并精确读回。首个查询进入相关搜索结果，但随后换词，结果列表仍旧；
  set-value 或 type/paste 后 Enter 均未证明新查询生效。原因未定位，不将搜索标为 verified。
  Alt+Left 未返回；按截图点击页内返回箭头后，首页及文件列表重新出现。

证据均在 `target/batch1-deep-20260928/`：`quark-drive.json`、`quark-search-result.json`、
`quark-empty-after-paste.json`、`quark-return-home.json` 及对应逐调用截图引用。
私人文件名、账号信息、截图不提交仓库。没有手动上传、下载、删除或分享文件。
新增 pack 将入口视觉路径标为已验证，搜索执行标为未验证；网盘文件操作留待后续隔离数据测试。

## 交付检查

`cargo xtask gate` 通过（file-lines、fmt、clippy、test、lint-packs、regression harness）；
应用档案校验为 10 packs、223 steps。日志：`target/batch1-deep-20260928/gate.log`。
四个场景脚本均通过 PowerShell AST 语法检查，`git diff --check` 通过。
新增网易云与腾讯会议脚本的停止状态离线测试：退出 77、零 CLI 动作、不清除停止标记。
上述停止标记仅位于隔离的测试目录，并非用户真实停止状态。
