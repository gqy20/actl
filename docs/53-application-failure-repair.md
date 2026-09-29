# 应用失败核对与修复（2026-09-29）

## 已证实的问题

### 关闭即时检查

Close-FixtureWindow 原先发送一次 close-window 后立即检查 HWND，窗口仍在退出时直接失败。
close-race.Tests.ps1 在模拟第三次观察后退出的场景复现失败（target/close-race-before.log）。
修订后只发送一次关闭，最多观察 5 秒；身份变化、停止、观察异常仍拒绝，不重复关闭。

### Office 关闭观察竞态

原生检查发现 Excel 空宿主后，再取 UIA snapshot 时窗口可能已消失，NOT_FOUND 使场景中断。
office-close-race.Tests.ps1 对旧脚本复现（target/office-close-race-before.log）；修订后仅针对
NOT_FOUND 重新核对原生 HWND，只有确认消失才成功；其他 provider 失败不吞掉。
轮询增加 100ms 间隔，避免 45 秒等待中连续启动大量 CLI。

## 已修订、待真机证实的定位与激活

- find 旧过滤路径为分配 ref 读取整树；无关节点的 control_type 失效可以阻断目标查找。
  严格 UIA 查询增加已知角色的原生 ControlType 候选筛选，并保留完整候选、属性核验及歧义拒绝。
  VS Code 脚本使用 role+name_contains 唯一语义查询，再使用返回 target；Word 正文使用严格 Document 查询。
  这不会把目标候选本身的失效变成成功，snapshot 原有错误也不会被隐藏。
- 四个应用脚本的启动归属检查改用原生窗口清单，避免无关顶层 provider 的 UIA 故障阻断归属判断；
  Zotero 仍核对可见 MozillaWindowClass、可执行文件以及隔离 profile/data 参数。
- VS Code/Office 启动后先按已核实 HWND 显式 focus-window，再定位输入目标。
  原 type 仅 SetFocus 控件，不能据此假定原生窗口已经获得前台；前台与具体焦点护栏继续保留。

## Zotero 的结论纠正

只读检查原失败目录 `target/app-verification-20260929/zotero-search-f8c5557010454d7881a84651207de350/data/zotero.sqlite`：
items 表有两条记录，itemDataValues 中包含 ACTL Alpha 中文测试与 ACTL Beta unrelated，中文码点正确。
所以该轮导入实际已经落库；界面 snapshot 未出现预期 item-tree 行，失败发生在列表显示/观察验收。
原失败记录保留，尚不能宣布搜索/导出通过，也不能盲目重复导入。下一次须在该隔离库只读观察列表和截图核对。

## 检查与阻塞

- role seed、候选属性失败和歧义回归通过；关闭与 Office 竞态回归通过。
- cargo xtask gate 通过：target/app-repair-gate.log。
- 新真机批次目录 `target/verification-98e5241727194f2cbaeb0a8f62637878`：首次 launch 在交接 25 秒超时，
  action_started=false，未启动 VS Code，整批已停止。需要用户确认首次开始后继续真机验收。
- 代码修订、自动测试通过不等于四个软件整轮验收通过。
