# 深层编辑与搜索测试

## 环境与隔离

2026-09-28，Windows 10.0.26200.0 中文桌面，VS Code 1.139.0、Zotero 10.0.3。
使用 `target/validation/release` 配套二进制；regress 报告保存哈希、调用输出和退出状态。
本轮使用 UIA 与键盘/剪贴板，不使用物理鼠标点击。没有进行跨日耗时排名。

VS Code 使用唯一 user-data-dir 与临时文本文件，禁用扩展。
Zotero 使用唯一 profile 和 data 目录、不登录同步账号，并在测试 profile 关闭自动同步和 Connector HTTP 服务。
独立配置和独立数据目录缺一不可，参见 [Zotero 多配置说明](https://www.zotero.org/support/kb/multiple_profiles)
与[多实例说明](https://www.zotero.org/support/kb/multiple_instances)。
`-datadir` 的绝对路径要求另从本机安装包 dataDirectory.js 核实，路径用 Join-Path 规范化。
首次启动可能重启进程，归属必须按新窗口、可执行文件、profile/data 命令行共同确认，不能只看 launch PID。

## VS Code

`scenarios/vscode-edit.ps1` 执行以下 11 项内容断言：

1. 三种输入（中英文、符号、多行），每种均验证输入保存、撤销保存、重做保存，共 9 项。
2. Ctrl+A 后全文替换并保存，与磁盘全文比较。
3. 关闭、重开同一测试文件，读取编辑器 TextPattern，与保存内容精确比较。

仅规范化 CRLF/LF；不以标题或命令 ok 代替内容终验。每次保存断言保留文件哈希。
初次脚本以裸文件名 wait 匹配到多个控件，已改为按文件名与 Edit 角色定位唯一编辑器。
第一轮通过证据：`target/vscode-edit-b9b5a55688a546f7a99e780805dac945/`，11 项通过，closed_verified。
焦点修复后的完整复验：`target/regress-5d7b7c814c9a4c61821b28a8a0fd8e5f/`，
仍为 11 项通过，closed_verified。

## Zotero 与焦点缺陷

合成两条 RIS 文献，通过文件菜单的“从剪贴板导入”送入独立库。
搜索测试覆盖英文 Alpha、中文测试、无结果关键词，每次分别断言实际条目行及搜索框读回，
再清空并断言两条原始文献恢复。未操作用户实际文献库。

旧二进制能输入 Alpha 并筛出目标，但 Ctrl+A/Backspace 后仍显示 Alpha，恢复失败。
在同一独立库重复后结果一致，截图确认搜索词未清空，非仅报告读取错误。
代码中 `ensure_element_focus` 拿物理焦点所在窗口标题做 `contains(pattern)`；
当 pattern 为 `hwnd:…` 时恒不匹配，随后去找窗口中第一个 Edit 强制聚焦。
Zotero 顶部通知含 Edit，实际搜索框的焦点因此被扰动。

先增加 `hwnd_focus_preserves_the_selected_field`，在旧匹配逻辑下失败；
再让物理焦点查询返回根窗口 HWND 与标题，HWND 选择器按句柄比较，标题选择器保持原匹配行为。
前台核对、键盘锚点及输入锁不变，不新增命令或协议字段。
原失败窗口在修正二进制下清空并恢复两条结果，通过后精确关闭。

诊断原始记录：`target/zotero-deep-32c610a27f2149a7b82b5a1c4bd2b5f7/`。
首次失败和后续修正的报告均保留，不覆盖成通过。

完整 Zotero 回归：`target/regress-78205f4163714f27a717b35f4c3e1be3/`，
10 项检查通过，closed_verified。前两次完整脚本试跑因 ref 漂移读回失败，保留原报告。
新通知在 find/get 之间出现时，`@e38` 实际变成“操作” Button，而新搜索框是 `@e44`。
因此重新 find 还不够，读回需要核对 name、role 和内容，使用有界的只读重查；不重放写操作。
本轮非空搜索词同时提供 ValuePattern 与 TextPattern，初轮空 properties 不能外推为控件无读取能力。
当前 ref 为遍历序号，不是持久控件 ID；这是后续提高动态界面写入可靠性需要补强的基础设施。

## 复现

```powershell
cargo xtask regress --case vscode-edit
cargo xtask regress --case zotero-search
```

两个用例均属 scenario，仅在本机安装对应软件、交互桌面空闲时运行。
离线门禁只运行停止状态下零 CLI 调用检查，不把第三方应用安装作为 CI 依赖。
失败及清理报告保留在独立 run 目录，涉及效果不确定或停止时保留窗口待核对。

本轮 `cargo xtask gate` 通过：fmt、Clippy、workspace 测试、9 个 pack/130 个步骤校验及
回归执行器离线测试全部完成；交互式专用测试仍按既有配置忽略，由上述场景实测另行覆盖。
