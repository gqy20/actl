# 浏览器迁移与首轮应用适配

## 范围与结论

2026-09-27，Windows 10.0.26200.0 中文交互桌面，使用配套的
`target/validation/release/actl.exe` 与显示端。浏览器报告保留二进制哈希和逐次调用记录。
本轮没有物理鼠标点击；VS Code 使用键盘粘贴和 Ctrl+S，其他写操作为测试页 UIA Invoke。

| 应用 | 版本 | 已验证 | 待验证或限制 |
| --- | --- | --- | --- |
| Edge | 154.0.4258.37 | 深树、中文正文、四档投影、动态内容稳定等待、歧义拒绝、归属清理 | 真实站点与扩展另走场景测试 |
| Chrome | 153.0.8010.53 | 内容测试同上，测试窗口关闭核验通过 | 不作为 CI 安装依赖 |
| 网易云音乐 | 3.1.41.205529 | 快照、Document 文本、截图核对 | 树仅 6 节点，搜索和播放控件操作待验证 |
| Zotero | 10.0.3 | 411 节点快照，快速搜索 Edit 定位 | Value 读回为空 properties，未写入搜索词；阅读器 text 仅工具栏/页码 |
| 腾讯会议 | 3.46.1.409 | 首页 91 节点快照和截图 | 只有系统窗口按钮提供可交互 ref；加入、预定、共享待验证 |
| VS Code | 1.139.0 | 364 节点快照，独立文件中文输入、保存、磁盘全文核验 | 当前用户工程及扩展界面未测 |

网易云树稀疏但 TextPattern 内容丰富，能读到推荐、歌单及歌曲信息。截图核对确认主要
搜索和播放区域没有对应的可用控件节点；不能把文本中的“播放”直接当作按钮。
Zotero 本轮未修改文献库；腾讯会议未加入会议或启用麦克风、摄像头、共享。

## 浏览器 fixture 迁移

`fixtures/browser-probe.ps1` 共用 PowerShell 生命周期；旧 Edge `.sh` 保留为转发入口。
每次使用唯一 profile、本地 UTF-8 HTML、页面标题和产物目录。启动时禁用扩展、使用私密窗口、
强制可访问性，仅作用于测试 profile。启动前检查停止状态，存在时零 CLI 调用、退出 77。

页面 Document 按唯一标题加角色查找；动态列表增长到第 40 项后读取确认。
稳定等待必须实际观察到内容变化，重名按钮必须即时返回 AMBIGUOUS。
关闭前核对 HWND、PID、进程启动时间、profile 和测试标签归属；额外网页标签出现则保留窗口。
Chrome 翻译弹层的语言标签位于另一个 Tab 容器，不误判为用户新增网页。
HTML、profile、JSON 与 stderr 均保留用于复核。

```powershell
cargo xtask regress --case edge-deep-tree
cargo xtask regress --case edge-content-view
cargo xtask regress --case chrome-content-view
```

三项最终运行均 passed，窗口均 closed_verified。

| 用例 | full 节点/字节 | skeleton 节点/字节 | content 节点/字节 | 组合节点/字节 |
| --- | --- | --- | --- | --- |
| Edge deep-tree | 289 / 13673 | 129 / 8706 | 37 / 2119 | 18 / 1409 |
| Edge content-view | 409 / 19927 | 170 / 11404 | 157 / 8333 | 59 / 4068 |
| Chrome content-view | 377 / 17480 | 149 / 9484 | 152 / 8125 | 59 / 4033 |

动态等待分别观测到 Edge 9 次、Chrome 14 次变化；这证明等待覆盖了实际加载过程，
不用于跨软件速度排名。字节数为本轮保存的完整 JSON 输出大小。

本地证据目录（不提交用户内容）：

- Edge deep-tree：`target/regress-cd491e1b0dae44bbb0c69820324157a4/`
- Edge content-view：`target/regress-afd67d702a604170ba444a0331750c2d/`
- Chrome content-view：`target/regress-9999af72f9da4f97ad9d41ad73a5688e/`
- 应用观察、截图及 VS Code 写入：`target/app-observe-d3d6ec1ca16541afb6b92ea61539febd/`

失败试跑的报告保留。此前 Edge 扩展欢迎页及 Chrome 翻译标签导致的清理拒绝，
均核对独立 profile 后精确关闭；修正后的完整重跑通过。

## 应用场景入口与下一步

`scenarios/app-observe.ps1` 按可执行文件路径识别窗口，多窗口不猜选。
已有窗口只观察；最小化窗口临时还原后恢复。新启动窗口记录归属，默认保留供后续测试；
指定 `-CloseOwned` 可在观察结束后关闭新建窗口。VS Code 无现有窗口时使用独立 profile 和文件。
这是人工场景入口，不会作为 CI 依赖或自动操作现有文档。

```powershell
# 由调用方将 ZOTERO_EXE 配置为本机已核实的 Zotero 可执行文件路径。
if (-not $env:ZOTERO_EXE) { throw 'Set ZOTERO_EXE to the verified executable path.' }
pwsh -NoProfile -File scenarios/app-observe.ps1 -App zotero -Program $env:ZOTERO_EXE -CloseOwned
```

`-App` 支持 cloudmusic、zotero、meeting、vscode；`-Program` 必须传本机已核实路径。
输出包含版本、窗口、节点统计和逐次调用。新建四个应用 pack，只将真正走通的路线标记 verified。
本轮网易云恢复原最小化状态；新开的 Zotero、腾讯会议、VS Code 窗口已核验关闭。

VS Code 写入试验先按临时文件名和 Edit 角色取唯一 ref，读取原文，再粘贴中英文并 Ctrl+S；
磁盘全文必须等于插入文本加原文，仅规范化换行。编辑器旁有 Chat Input，禁止用裸 Edit 猜目标。
后续优先把此闭环做成独立场景 fixture，再测 Zotero 独立测试库的搜索与恢复。
网易云先用文本读取做可核验场景；腾讯会议需要先解决主页操作入口的定位，再测会议功能。

## 执行器修正及离线覆盖

GUI 子进程可能继续持有 fixture 的 stdout/stderr，即使脚本已退出，外层收集仍会等待。
先用“子进程持有管道 6 秒”的离线测试复现，再把脚本退出后的日志排空限制为 1 秒。
`*.capture.json` 的 `streams_drained` 标记是否完整排空，并记录真实退出码；报告引用该文件。
未完整排空不能视为日志完整。该修正不等于整个 fixture 有强制超时或取消协议。

门禁覆盖旧脚本迁移约束、双浏览器停止前置检查、翻译标签与真实额外网页标签区分、
GUI 继承管道退出行为；真机验收通过不替代这些离线回归。
