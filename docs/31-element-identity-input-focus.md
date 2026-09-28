# 控件身份绑定与输入焦点验收

## 本批实现

- snapshot/find 的本地记录保存同次 DFS 中每个 ref 的 RuntimeId、PID、role、name、AutomationId。
  skeleton/content 投影沿用原编号。落盘失败返回错误，避免给出无法重放的成功快照。
- click/type/set-value/get 带 `--snapshot` 时，在实际 locate 命中处检查窗口和控件身份。
  缺记录、缺身份、序号指向另一个对象或元数据改变返回 STALE_REF；不自动将旧 ref 转成名称写入。
  不带快照的 ref 保持兼容的序号语义。语义选择器仍每次重新查找，唯一性规则不变。
- type 设置焦点后校验具体 UIA 实例，原生前台与物理焦点锚点继续生效；注入边界复核。
- `press --target` 只断言目标已经聚焦，不猜测首个 Edit，也不抢回焦点；可通过 `--snapshot` 绑定 ref。
- `type --verify-value` 在原控件上最多只读等待 2 秒，精确比较完整值。
  不可读/不匹配返回 ASSERTION_FAILED，实例改变返回 STALE_REF，保留 delivered 信息，不重放输入。
- 所有能力使用既有同步模型与依赖，没有新增 unsafe。键盘仍按 docs/06 使用显式 --app，
  物理鼠标仍要求 --physical。

具体参数、错误与新增输出字段见 [协议](06-design-blueprint.md)末尾补充。

## 复现与离线验证

- 首先复现旧代码对空窗口 RuntimeId 放行的问题：
  `target/element-identity-red.log`。修复后同一测试通过。
- 身份测试覆盖同序号同名但 RuntimeId 变化、PID 改变、虚拟列表行名称变化、旧记录无身份、
  焦点缺失、命令作用域恢复。错误 envelope 和 type 成功 envelope 有 golden。
- CLI 契约验证新参数可发现，press 的 --snapshot 必须搭配 --target。
- 停止状态下记事本与夸克场景应退出 77、零 CLI 操作，不清除用户停止标记。
- 完整门禁日志：`target/identity-gate-final.log`。

## 记事本：系统 fixture

新 fixture：`fixtures/input-target.ps1`，注册为 `cargo xtask regress --case input-target`。
独立文件、已有记事本会话保护、窗口身份核验与已保存测试标签的定向关闭；失败证据保留。

debug 验收报告：`target/input-target-e494f79582e9466ea091469d903741ab/result.json`。
5 项检查通过，cleanup 为 closed_verified：

1. skeleton 快照 ref 绑定，输入并读回完整文本。
2. 同一窗口中的另一 Button 不是当前焦点，拒绝向其发 Enter。
3. 零长度输入配错误期望值，返回 ASSERTION_FAILED，保留输入事实。
4. 仅修改此 fixture 自己的快照记录以模拟序号被另一实例占用，get 返回 STALE_REF。
   这项是合成身份漂移，不宣称真实 GUI 在本轮发生了相同序号复用。
5. 拒绝路径后正文和已保存文件仍是预期内容。

发布版通过正式入口的首次回归保留为失败：
`target/regress-98d4eb73d64d4f0a9644b7ab0abb749b/report.json`。
type 时物理前台不匹配，PERM_DENIED 明确记录 no text was sent；随后 UIA 与截图确认测试文档为空。
没有修改输入护栏；fixture 增加显式 focus-window 准备。
在核对原 HWND/PID/启动时间之后，独立发布版确认通过快照绑定、输入读回、磁盘保存和测试标签关闭：
`target/input-release-confirm-7ee180e1f45642a3bbb434dc29d98fc8/result.json`。
此确认只覆盖这些步骤，不将原始整轮回归改写成成功。

截图还发现启动记事本会恢复用户旧标签，单独 Document 数量不足以证明整个窗口归属。
早期 debug 报告的 closed_verified 只代表 HWND 关闭，不是标签隔离验收。
最终 fixture 改为核对未修改的任务 TabItem、查找其子 CloseButton、绑定快照点击并确认该标签消失；
旧标签保留。离线用例覆盖另一个已修改的用户标签，防止误选它的关闭按钮。
本次恢复的旧测试标签也已按该路径清理，证据：
`target/input-restored-cleanup-1b172f59de574a7b910c2513b4feb7b2/result.json`。

## Zotero：独立库

`scenarios/zotero-search.ps1` 为输入与读回增加快照绑定；输入增加完整值校验；
清空和 Tab 提交增加目标焦点断言。

首轮 `name:快速搜索` 同时匹配搜索框和“快速搜索模式”按钮，返回 AMBIGUOUS，未发键。
保留原报告：`target/zotero-search-49a50dd070ac47a1b9c25daebda0aed2/result.json`。
场景改为 find 限定 Edit 后，使用当次 ref 与 snapshot_id，不放宽定位规则。

复验：`target/zotero-search-215ef7710b134a7fb4ea34d7ff4b90d3/result.json`。
11 项检查通过：导入两条合成文献、三组搜索及读回、每轮恢复两条记录、元数据编辑与 CSL JSON 导出核验。
独立 profile/data，cleanup 为 closed_verified。

## 夸克：双输入面

证据目录：`target/quark-input-guard/`。本轮新标签页左侧网盘入口通过截图、hover 核对与显式
physical 点击进入；UIA 与截图确认网盘页面。未上传、下载、删除或修改备份设置。

本轮 Ctrl+F 后聚焦的是网页“搜网盘…” Edit，而不是原生 `view_1012`。
旧记录中原生输入框路线只能作为历史观察，不能硬编码为固定规则。

- `native-target-refusal.json`：期望原生 Edit、实际网页 Edit，返回 NOT_ACTIONABLE，未发键。
- `web-type.json`：绑定网页 ref 输入成功，focus_verified/value_verified 均为 true。
- `submit.json` 与 `route.json`：回车前焦点验证成功，Document URL 的 keyword 与查询一致。
- `result.png`：带多个英文词的长查询仍返回部分匹配，不能因为带随机后缀就断言空结果。
- `empty-result-text.json`：换用无意义单词后，路由、0 项和“暂无搜索内容”一起验证。

最初名为 `wrong-target.json` 的探测实际成功发键：当时网页 Edit 就是正确焦点，
该文件不能算作拒绝证据。后续使用原生目标才验证了错误焦点拒绝，保留全部原始观察。

新场景 `scenarios/quark-search.ps1 -Hwnd <已观察的网盘句柄>` 要求现有登录页面，
只验证空结果搜索，不推断任意查询的匹配数量。菜单和建议变化后重新 find，禁止复用旧序号。
报告：`target/quark-search-8fbd63b4cf0a4f1f98eff4656b582a28/result.json`，两项检查通过。
网盘窗口保留，截图与文件名证据仅保存在本地 target。

## 限制

RuntimeId 不是跨启动永久标识；provider 若复用相同身份与全部相同元数据，本批无法辨认业务对象变化。
采样、最后核验和系统注入不是原子事务，不能承诺消除最后检查之后的外部竞态。
不可靠的 UIA 焦点会被拒绝，本批不以仅窗口匹配降级。控件重建后调用方重新观察、
使用唯一语义选择器定位，不自动重试已可能生效的写操作。
计算器 UWP 宿主关闭生命周期仍是 docs/30 的独立遗留项。
