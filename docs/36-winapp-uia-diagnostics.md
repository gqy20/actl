# 36 · winapp 与 Inspect UIA 对照诊断与优化

日期：2026-09-28。状态：外部工具安装与资源管理器只读冒烟已验证；
actl 对照测试、复杂应用覆盖及功能优化尚未验收。

## 1. 用途与边界

微软 Windows App Development CLI 的 `winapp ui` 可直接查询 Windows UIA。
把它作为 actl 的外部诊断工具和实现参照：当 actl 的树、属性、窗口或焦点结果不符合预期时，
用另一客户端读取同一现场，帮助定位差异。它不是 Inspect.exe 的命令行模式。

- Inspect.exe 是 GUI 检查器，可人工检查 UIA/MSAA 和 Raw/Control/Content View。
- winapp 提供 CLI 与 JSON 输出，适合留证和重复对照。其查询使用 Control View；
  不应将它和 actl 原始树、skeleton 或内容投影的节点数直接比较。
- 两者都依赖应用暴露的信息；不能把自绘画布、网格顶点或游戏场景自动转换为 UIA 控件。
- winapp 不加入 Rust 依赖、发布包或 CI 必装项；不改变 actl 的协议、默认输入方式和交接契约。
- winapp 的输入占用机制不保证与 actl 互通。禁止用它绕过 actl 的急停、人工让行、
  焦点拒绝或权限拒绝；先完成诊断，再按原有授权和护栏处理写操作。

## 2. 本机安装与证据

本轮通过 PATH 中的 `winapp.cmd` 调用工具。其他环境用 `Get-Command winapp`
确认入口，版本以 `winapp --version` 为准。程序与配套 DLL 必须保留安装目录结构。
对照采样脚本需要真实的 `winapp.exe`，不能传入 cmd 包装器；通过 `WINAPP_EXE`
配置或从 PATH 查找 exe，示例见 [37](37-uia-observation.md)。

2026-09-28 从官方 v0.7.0 release 下载 `winappcli-x64.zip`，校验 SHA-256：

```text
ZIP: 236e35173f85a88a62cb030bdc07db6590fb7790514d530a050482a0da80313b
EXE: 92cc1299d79f422d8e2d6467fa6eb6385390bf70cd0df960dabb9db1a908212d
```

官方仓库当日 1,281 Star；本次按用户明确指定安装处理，并记录于本机工具清单。
这不是对后续任意低于门槛工具的安装授权。

### 已验证的调用

| 调用 | 结果与限制 |
|---|---|
| `winapp --version` | 返回 0.7.0；首次调用额外输出欢迎及遥测说明，不能假定所有 stdout 都是纯数据 |
| `winapp ui --help` | UIA 子命令可用；后续还核对了 inspect、list-windows、get-property、get-focused 帮助 |
| `winapp ui list-windows -a explorer --json` | 退出码 0，返回窗口清单；包含辅助窗口，需核对 class、尺寸、PID 和 HWND |
| `winapp ui inspect -w 44961912 --depth 5 --json` | 退出码 0、JSON 可解析；单个资源管理器窗口返回 100 个元素 |

最后一条的 HWND 仅为历史证据，不能在后续会话复用。输出记录 `windowDpi=192`、
`scale=2`、`coordinateSpace=physical-screen-pixels`，包含名称、控件类型、AutomationId、
类名、边界、启用/离屏状态及 children。100 是该深度下的样本数，不是完整覆盖率。

环境补录：Windows 11 家庭版中文版，系统版本 10.0.26200，Build 26200。
原始结果已复制到本地 `target/winapp-diagnostics/2026-09-28/explorer-inspect.json`，不提交业务原始树。
文件 SHA-256：`9e5136760f8a1623c40e8d62f5c47871d44cbce7cec513bdfcad7d04f7b2446e`。
此历史样本没有成对的 actl 调用及完整逐调用日志，不充当性能或差异验收。

本次 winapp 测试没有写入应用、创建业务窗口或关闭用户窗口。
此前 Inspect GUI 已启动但没有完成目标属性检查，窗口保留；Chrome 的辅助 Computer Use
检查被其 URL 判定限制中断。这些受阻记录不因 winapp 冒烟成功而改记通过。
Blender、Photoshop、Minecraft 尚未通过本工具实测。

## 3. 何时使用与操作流程

遇到以下情况，优先追加一次有界的只读对照：

- actl 只看到窗口/大面板，但画面存在输入框或按钮；
- 同名控件定位歧义，需要父容器、AutomationId 或 ClassName 证据；
- UIA 值、当前焦点、离屏状态与实际操作结果矛盾；
- 控件重建、弹窗或窗口归属导致定位失效；
- 计划增加属性/Pattern 诊断，或评估树投影是否丢失关键节点。

先按现有诊断纪律检查桌面环境、窗口状态和必要截图。不要仅凭一次空树就认定应用不支持 UIA。
确认版本与命令帮助后，按进程列出窗口，从本次结果选择精确 HWND：

```powershell
winapp --version
winapp ui inspect --help
winapp ui list-windows -a explorer --json

# 下列变量由调用方填入刚核对的窗口与控件；不是可直接盲跑的脚本。
winapp ui inspect -w $targetHwnd --depth 5 --json
winapp ui get-property $selector -w $targetHwnd --json
winapp ui get-value $selector -w $targetHwnd --json
winapp ui get-focused -w $targetHwnd --json

# 在同一现场，用当前被测 actl 二进制执行：
actl snapshot --app "hwnd:$targetHwnd"
```

后四种诊断路线中，属性/值/焦点的业务读取仍需逐场景验证；帮助存在不等于已通过。
需要更深层时小幅增加深度或限定子树，不从桌面根无限遍历。使用外部进程超时边界，
超时后保存错误，不将截断输出当成完整树，也不反复重启目标应用。

每次采集保存命令、时间、退出码、stdout、stderr、工具版本和二进制哈希；
记录系统/应用版本、窗口 PID/HWND/进程实例、DPI、前台/最小化/模态状态、树深度和过滤项。
stdout 单独保存并解析；JSON 失败、超时和权限错误分别记录。资料放本地 target，提交文档只摘必要且脱敏的证据。

对齐同一窗口和同一局部控件：角色、名称、AutomationId、父级路径、边界和当前值。
winapp 的 selector/slug 不转换为 actl `@eN`，也不当成永久身份。
回到 actl 后重新观察，用其语义选择器或绑定 snapshot_id 的 ref 核验目标。

### 3.1 Inspect.exe：安装位置与启动

Inspect 随 Windows SDK 提供。微软将其标为旧工具，推荐 Accessibility Insights；
这里保留 Inspect 用于已有环境下的 UIA/MSAA 交叉检查。
它是 GUI 工具，官方未文档化用于查询控件树或导出 JSON 的 CLI 接口。
从 PowerShell 启动只会打开窗口；批量采集使用上面的 `winapp ui`。

本机 2026-09-28 已核对：

| 项目 | 版本与状态 |
|---|---|
| SDK 安装包 | 包含 winsdksetup.exe、Installers 和 Redistributable；与 SDK 安装后的工具目录区分 |
| 新版 x64 Inspect | SDK 10.0.28000.0，文件存在，微软数字签名有效 |
| 旧版 x64 Inspect | SDK 10.0.26100.0，仍保留，签名有效 |

SDK 安装包标识为 10.0.28000.2705，安装目录为 10.0.28000.0；
Inspect 自身 FileVersion 为 7.2.0.0。三者口径不同，记录证据时分别保存。
两版均发现 x64/x86/ARM64 文件，本机检查优先使用 x64 版本。

从 SDK 注册位置列出本机已安装的 x64 Inspect：

```powershell
$kitsRoot = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' `
  -Name KitsRoot10 -ErrorAction Stop).KitsRoot10
Get-ChildItem -LiteralPath (Join-Path $kitsRoot 'bin') -Directory | ForEach-Object {
  $candidate = Join-Path $_.FullName 'x64/inspect.exe'
  if (Test-Path -LiteralPath $candidate -PathType Leaf) { $candidate }
}
```

从结果中选择所需 SDK 版本，将完整路径赋给 `$inspectExe`，再用 `& $inspectExe` 启动。
注册项缺失或未找到文件时，先核实 SDK 是否安装及实际目录；不假设固定盘符或版本目录。
普通检查无需默认以管理员启动；权限异常先确认目标完整性级别和工具状态，不盲目提权。

### 3.2 Inspect 的只读检查步骤

以下步骤依据官方说明整理，尚未在本机完整走通，不能当成已验证路线直接盲跑。

1. 打开 Inspect 和目标应用，确认要检查的是正确窗口与进程；避开输入、保存、发送等业务操作。
2. 选择 **UI Automation Mode**。通过 **Watch Cursor** 跟随鼠标，或 **Watch Focus**
   跟随键盘焦点定位目标。将光标移回检查器可能使目标变化，需要按现场关闭跟随再读取。
3. 开启 **Show Tree** 查看父子结构，按需使用 **Show Highlight Rectangle** 核对控件边界。
4. 查看名称、ControlType、AutomationId、ClassName、BoundingRectangle、启用/离屏/焦点状态，
   以及 Value、RangeValue、Text、Invoke、Selection 等 Pattern 的支持信息。
   属性缺失和 Pattern 不支持分别记录，不能推断默认值或操作成功。
5. 在同一目标上切换树视图：**Control View** 用于与 winapp 对照；
   **Raw View** 检查被控制视图过滤的节点；**Content View** 检查内容相关投影。
   原始视图仍只包含 provider 暴露的元素，不保证覆盖所有画面内容。
6. 怀疑旧式可访问性接口时切换 **MSAA Mode**，检查其名称、角色、值和层级。
   MSAA 与 UIA 的结构和能力可能不同，MSAA 可见不代表 actl 当前可以直接操作。
7. 保存目标截图、所选模式/视图、属性与层级摘录，和同一现场的 winapp/actl 输出一起分析。
   标注采样时间与目标身份，避免把 Inspect 自身获得的焦点误认为业务输入框焦点。

**Action 菜单中的 Invoke、SetValue、Select、Focus 等会操作目标，属于单独的动作验证，
不包含在只读解析里。** 执行前需要符合当前任务授权与桌面护栏；Pattern 声明支持只说明
接口可查询，仍须核验实际结果。急停、人工接管或焦点拒绝时不切换 Inspect 继续写入。

### 3.3 与 winapp、actl 配合及故障判定

| 需求 | 工具与验证方式 |
|---|---|
| 重复导出控件树和属性 | winapp JSON 留证，与 actl 在同一窗口、视图及状态下对照 |
| 查看控件在画面上的位置与父子关系 | Inspect 树及边界高亮，记录模式和视图 |
| Control View 缺项 | Inspect Raw View/MSAA 补查；排除深度、过滤及 provider 差异 |
| 验证 actl 定位修复 | 回到 actl 重新观察、绑定身份，运行复现测试和实际场景验证 |

此前本机测试已确认新版 Inspect 可以启动，但属性区没有形成有效目标数据；
辅助 Computer Use 读取 Inspect 自身时仅返回窗口和标题栏，窗口交互也没有得到有效验证。
这仅能证明本轮检查流程受阻，不能据此断言 Inspect 不兼容或目标应用不暴露 UIA。
资源管理器控件树曾通过辅助 UIA 通道读取，随后 winapp 单独读取成功；两者均不记作
Inspect 的通过证据。Inspect 当轮保留现场，未自动关闭。

遇到空白或不更新时，先检查桌面、目标窗口、所选模式、跟随设置和工具响应，再核对截图；
通过另一只读客户端交叉查询后，仍未知则保留错误与现场，不循环抢焦点或反复启动工具。

## 4. 如何把差异变成优化

| 现象 | 先核对 | 可以形成的改进 |
|---|---|---|
| winapp 有控件，actl 没有 | 同一窗口、树视图、深度、投影、加载时刻 | 补最小复现，修遍历或投影遗漏；不是直接扩大所有默认快照 |
| 两边都有控件，actl 定位不唯一 | 同名项、父容器、角色与 ID | 优先现有组合定位；真实缺口再评估字段扩展 |
| 数值/文本结果不同 | Value/Text 通道、文档范围、格式、读取错误 | 改进通道诊断和错误表达，避免把失败读成空值 |
| 焦点或窗口结果不同 | owned 弹窗、最小化、窗口实例、工具自身干扰 | 补窗口/焦点证据，验证现有护栏；不能据外部成功放宽护栏 |
| 都只返回面板 | Raw View/MSAA、权限、应用就绪及可访问性支持 | 记录 UIA 边界，评估应用原生 API 或上层视觉路线 |
| 支持 Pattern 但动作无效果 | 真实状态变化及业务后置条件 | 保留假成功记录；动作验证需授权的独立场景 |

优化顺序：只读观察与错误诊断 → 定位与身份 → 场景动作验证 → 有需求再评估新输入能力。
winapp 的 pen/touch、录制和沙箱能力仅作研究线索；本次没有引入这些能力，
也没有授权绕过 `--physical` 或把外部程序接入 flow。MCP、视觉定位、应用适配器范围仍以 PRD/ROADMAP 为准。

发现 bug 先补复现测试，再改代码；协议变更同步 golden、docs/06 和 CHANGELOG。
UIA 写入与物理注入的成功率、耗时只在同环境下比较，不能用命令数量或单次读取速度推定可靠性。

## 5. 复杂应用的观察清单

| 应用 | 优先检查 | 需要应用原生数据的部分 |
|---|---|---|
| Blender | Outliner、Transform、修改器面板、编辑模式工具、菜单 | 顶点/边/面选区、拓扑、修改器求值结果；结合 bpy/RNA 属性和命令映射 |
| Photoshop | 图层面板、数值栏、工具选项、模态对话框 | 像素、蒙版和图层数据；结合 UXP DOM/batchPlay |
| Minecraft | 启动器、菜单、聊天输入；分 Java/基岩版记录 | 世界坐标、方块及建筑验证；参见 [17](17-game-scenarios.md) |

以上是未验收的检查范围。分别记录可观察、可定位、可读值、可执行、可核验五项，
不能把“树里有名称”当成整套精修流程可用。验证后的应用知识进入 pack；
pack steps 仍只写 actl 命令，外部 winapp 采样方法留在本专题。

## 6. 参考来源

- [官方仓库与 v0.7.0 发布包](https://github.com/microsoft/winappCli/releases/tag/v0.7.0)。
- [winapp UI Automation 官方文档](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/ui-automation)。在线内容可能先于本机版本，以本机 help 和实测为准。
- [Inspect 官方文档](https://learn.microsoft.com/en-us/windows/win32/winauto/inspect-objects)。
- [Blender Python API 入门](https://github.com/blender/blender/blob/main/doc/python_api/rst/info_quickstart.rst)。
- [Photoshop batchPlay 与动作记录](https://developer.adobe.com/photoshop/uxp/ps_reference/media/batchplay/)。
