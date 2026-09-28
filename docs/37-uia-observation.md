# 37 · UIA 读取状态与 winapp 对照采样

日期：2026-09-28。承接 [36](36-winapp-uia-diagnostics.md) 的第一批实现。

## 读取契约

新读取模块通过 UIA Pattern availability 属性判定是否支持，再获取 Pattern 及其值。
availability 读取错误、非布尔返回、已声明支持但获取/读取失败都属于观察失败；不能视为不支持。
成功的空字符串始终保留。ValuePattern 不支持时才允许 TextPattern 回退。

默认 get 输出 `unsupported` 数组；显式请求无可用通道返回 NOT_ACTIONABLE。
UIA_E_ELEMENTNOTAVAILABLE 返回 STALE_REF，E_ACCESSDENIED 返回 PERM_DENIED，其他失败返回 INTERNAL，
保留通道及原生错误。verify 与 wait 不把读取失败改成 ASSERTION_FAILED/TIMEOUT；稳定性等待不能用失败样本确认稳定。
输入后读回失败保留 delivered、value_verified=false、retry=never，不重放输入。

对原 `.filter(|v| !v.is_empty())` 通道选择提取复现测试，两个空值案例在修复前失败，
日志 `target/uia-read-before.log`。当前测试覆盖空值、错误不回退、availability 失败、Pattern 不支持、
stale/权限错误和输入送达证据；golden 覆盖空值成功输出及不支持错误 envelope。

## 采样脚本

```powershell
# WINAPP_EXE 可指定真实 exe；未设置时从 PATH 查找 winapp.exe。
# $targetHwnd 由调用方填入本次发现并核对的窗口句柄。
$winappExe = $env:WINAPP_EXE
if (-not $winappExe) {
  $winappExe = (Get-Command winapp.exe -CommandType Application -ErrorAction Stop).Source
}
pwsh -NoProfile -File scenarios/uia-compare.ps1 `
  -Hwnd $targetHwnd `
  -Winapp $winappExe
```

可指定 `-Actl <被测actl.exe>`、`-Depth 5`、`-TimeoutMs 10000` 和新的 `-OutputDirectory`。
直接调用 exe，避免 cmd 包装器使超时后留下未受控子进程；不安装外部依赖。
若 PATH 只有 `winapp.cmd` 包装器，应配置 `WINAPP_EXE` 指向其实际调用的 exe。

顺序：版本/哈希 → 原生窗口与进程实例 → actl snapshot → winapp inspect → actl snapshot → 窗口与进程实例复核。
每次保存参数、时间、退出码、stdout/stderr；流量分别排空，超时、非零退出和 JSON 错误保留原输出。
超时只结束本次启动的诊断 CLI，不关闭业务窗口。急停存在时退出 77 并保留标志；不恢复最小化窗口、不抢焦点、不启动业务应用。

报告 `collection_status=collected` 只表示采集完成，不代表两工具结果相等或 UIA 完整。
`observed_structure_unchanged` 只比较 actl 两次角色/ID/名称/深度序列，不能证明采样期间没有变化。
winapp 为 Control View，actl 为 Raw/full；深度和投影不同。报告始终要求人工分析，保留 DPI、窗口状态和截断标记；不自动将数量差异判为缺陷。

## 检查入口与边界

`cargo xtask gate` 包含新增 Rust 测试及 `crates/xtask/tests/uia-compare.Tests.ps1`。
脚本测试用受控 CLI 验证大量 stderr、空字符串、超时、坏 JSON、非零退出和急停前置拒绝；不操作业务应用。
最新门禁日志：`target/uia-observation-gate-final.log`。真机记录将在本节追加，原失败记录保留。

### 本轮结果

- 最终 `cargo xtask gate` 通过：219 项测试通过、4 项忽略，含采样脚本的进程/故障/急停测试。
- 被测 actl：`target/validation/release/actl.exe`，SHA256 `3718FB1C747A84187A840D8155246B5E9238CF520F08A81703B0758028B016A3`；未替换 PATH 中的安装版本。
- winapp：0.7.0，SHA256 `92CC1299D79F422D8E2D6467FA6EB6385390BF70CD0DF960DABB9DB1A908212D`。
- 在当前系统任务栏 HWND=65996、explorer PID=16560 上完成 A/B/A 采样。actl 两次各 4 个节点、winapp 3 个节点；actl 序列一致且未截断。winapp 报 DPI=192、scale=2、physical-screen-pixels。采样完成，不据节点数差断言缺陷。
- 原始参数、输出、环境、窗口状态和进程实例见 `target/uia-compare-taskbar-20260928/report.json`；没有创建或关闭业务窗口，没有输入/焦点操作。
- 在该轮观察到的唯一 ID=40965 Pane 上验证：get name 返回空字符串；默认 get 列出 value/text 不支持；显式读取 text 返回 NOT_ACTIONABLE；verify name 的空字符串断言通过。四项局部检查见 `target/uia-read-live-20260928-corrected/report.json`。
- 第一轮最后一条 verify 调用参数顺序写错，返回 PROTOCOL；原始四次调用及失败说明保留在 `target/uia-read-live-20260928/`。后续正确调用单独存档，未将第一轮改记通过。

资源管理器窗口当时均为最小化，未自动恢复。计算器/记事本输入框的空 ValuePattern/TextPattern、真实控件重建与 provider 故障尚未实测；上述任务栏局部结果不替代这些验收。

本轮未增加 ClassName 组合条件或完整 Pattern 能力面板；未替换底层 UIA 引擎。
上述第一轮未处理旧 snapshot 的部分属性/树遍历和表格结构回退；后续变更见下节，不据局部修复宣称所有观察路径严格。
完整 Raw/Control View 对齐、控件 RuntimeId 跨客户端配对及复杂应用回归仍待后续实现。

## 第二批：观察完整性

snapshot 与旧定位链共享可失败的 RawView 子级枚举，深度与节点截断分别标记；
属性、子树或必要身份读取失败直接报错。find、ref 重放与表格树回退不使用截断范围证明完整性。
窗口标题解析、距离锚点及虚拟化查询保留读取错误，元素等待仅把 NOT_FOUND 当作缺席。
表格只在明确不支持 GridPattern 时回退并报告原因；空表、空单元格保留，读取失败不回退。

- 提取旧遍历语义后的三项复现测试均失败：深度截断漏报、恰好到上限误报、枚举失败吞并；日志 `target/observation-before.log`。
- 新测试覆盖上述问题、后续分支失效、截断拒绝的错误 envelope、表格部分失败/空值/非法规模与回退契约。协议见 [06](06-design-blueprint.md)。
- 只读系统任务栏冒烟完成：两种 snapshot 入口、完整性字段、不存在目标的 wait --gone、前后窗口 PID 与进程启动时间核对。4 个节点，无截断。报告 `target/observation-integrity-live-20260928-164829/report.json`。
- 该次被测二进制为 `target/validation/release/actl.exe`，SHA256 `C64A6F6C857C24B06435DC5CD1C2F491CA8A49F6FF893527D71174A732DB197D`。未替换 PATH 安装版本，未创建/关闭业务窗口或注入输入。
- 首次脚本错误地要求 visible 字段存在，而原生窗口输出在 true 时省略该字段，因此未选中目标；修正脚本后另开一轮，原失败报告保留于 `target/observation-integrity-live-20260928-164752/report.json`。
- 最终门禁 `cargo xtask gate` 通过：228 项测试通过、4 项忽略，日志 `target/observation-integrity-gate-final.log`。
- ID 快速查询补齐显式 RawView 后，最终二进制 SHA256 为 `FE59EA44BC794C78671092C0A708F8C3F48432CCE65222E6D96737A8B2DE8CE0`。重新执行上述只读冒烟并增加唯一 ID 的 name 读取，全部通过；逐调用证据为 `target/observation-integrity-live-20260928-165205/report.json`，早期结果及哈希仍按原轮次保留。

该样本不覆盖真实表格、深树截断、控件重建、虚拟化多匹配或 provider 故障。
单次 COM 调用仍可能阻塞；节点和深度预算不能替代硬超时。树扁平化也不保证逻辑表格列结构完整。
Pattern 能力详情、局部展开、性能批量缓存及其他旧窗口观察路径留在 ROADMAP。

## 第三批：窗口清单迁移

窗口清单、RuntimeId 基线、新窗口等待和 PID 定位改用可失败的 RawView 枚举；
必要属性失败不再被 `.ok()` / 默认值吞并。抽取旧语义后，根失败、子枚举失败、
分类失败和 PID 读取失败的四项复现测试均失败，修复后通过；`wait --idle` 同步修复
系统查询失败被当为空闲的问题，并在每次轮询检查流程暂停/停止。
复现日志及本机只读结果见 [44](44-phase2-acceptance.md)。本批不改变完整 Raw/Control
View 对齐、局部展开和 Pattern 面板的待实现状态。
