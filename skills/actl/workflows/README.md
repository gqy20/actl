# actl 业务流程

本目录存放 actl 技能内部的可复用业务流程 YAML。统一入口是上级 [SKILL.md](../SKILL.md)，
不为每个任务创建独立 skill，也不另设仓库根目录 workflows。

## 职责与位置

| 位置 | 职责 |
|---|---|
| `skills/actl/SKILL.md` | 引导 agent 选择流程、核对所需信息和执行边界 |
| `skills/actl/apps/` | 单个应用的定位知识、操作路线与已知限制，供 agent 阅读 |
| `skills/actl/workflows/<slug>.yml` | 一个完整业务目标的执行计划，可涉及一个或多个应用 |
| `fixtures/flows/` | 流程引擎的最小测试用例，不作为业务流程目录 |
| `scenarios/` | 真实应用验收入口说明；执行实现位于 actl-devtools |
| `skills/actl/references/` | 随技能分发的使用说明 |
| 仓库 `docs/` | 完整协议、设计与验收证据，不随技能分发 |
| `target/` | 本地测试输入、日志与证据，已被 Git 忽略 |

运行状态由引擎保存到协议规定的运行目录，不回写流程 YAML；见
[流程使用说明](../references/flows.md)。引擎按传入的文件路径执行，
不依赖本目录布局；流程也不会因放进 skill 就被自动发现或自动执行。

## 流程组织与登记

- 文件名使用小写英文和连字符，例如 `share-recent-music.yml`；按业务目标命名，不按软件切断一个完整任务。
- 具体计划使用当前协议支持的字段。业务流程不是应用 pack，不使用 `pack_format/routes`，不由 `lint-packs` 代替流程校验。
- 新增文件时，在下方索引登记用途、必需信息、应用前提、校验状态和真机证据链接。协议校验成功与业务验收成功分别记录。
- agent 先读索引及相关应用档案，核对用户授权、接收人等关键条件，再准备执行计划；控件身份需在执行现场重新观察。
- 公共文件不写真实联系人、账号、私人路径、固定 HWND/ref 或本次运行的坐标。个人任务的具体计划和输入留在本地，不提交到技能目录。
- 当前 v1 支持只读步骤的显式输出保存和后续完整参数绑定；绑定无法解析时停止，不执行占位值。支持即时只读条件 when/unless/otherwise；未知结果停止，已派发路线恢复先核验。不支持参数插值、嵌套分支、循环或子流程调用。任务输入通过 inputs 声明及 --inputs 文件传入，args 中使用 {input: name} 绑定完整参数；解析后统一预检，恢复读取保存的具体计划。
- 应用档案供 agent 参考，当前引擎不支持 YAML 自动引用 pack 路线。未来加入复用能力前须先修订协议、测试和文档。
- 对会过期的业务输出设置 `max_age_ms`。恢复报告 `output_freshness_failed` 时，先核对已完成动作，再准备后续计划；不要修改保存时间或盲目从头重跑。时间未过期仍需 before/verify 核验目标身份和业务状态。
- 需要恢复前核对当前页面、会话或接收人时声明 `resume_check`；首次执行条件仍放 before。只读目标失效可设 `retry: stale_read`，最多重新查询三次。`result_observation_failed` 表示结果尚不明确，不能通过 --retry-step 绕过；先解决观察问题。用法见 [流程恢复说明](../references/flows.md#暂停与恢复)。

## 流程索引

| 流程 | 用途 | 校验状态 |
|---|---|---|
| [ensure-app-window.yml](ensure-app-window.yml) | 输入 app/program；app 为唯一标题片段，先启动或复用，再独立核对前台 | Chrome 隔离本地页组合实测通过，测试计划显式补 profile/page 启动参数；不提供关闭与归属证明 |
| [dismiss-optional-control.yml](dismiss-optional-control.yml) | 输入 app/dialog/dismiss；dialog 为严格 UIA 选择器，窗口保留，存在才点击并核验消失 | Chrome 本地页存在/消失跳过实测通过；其他应用分别验收 |
| [ensure-element-value.yml](ensure-element-value.yml) | 输入 app/target/value；目标支持 ValuePattern，已达目标值则跳过 | Chrome 本地页写入/读回/已达标跳过实测通过；出现一次 60 秒查询后重查成功，见 docs/64 |
| [vscode-resume.yml](vscode-resume.yml) | 隔离文件启动、一次输入、完整文本核验；输入 program/profile/file/app/target | 参数预检测试；旧生成计划真机通过，新模板本轮待验收 |
| [verify-text.yml](verify-text.yml) | 已打开应用的完整文本只读核验；输入 app/target/expected | 参数预检及 VS Code 真实全文只读核验通过；见 docs/56 |
| [edit-value.yml](edit-value.yml) | 核对 previous 后 set-value，再核对 value；输入 app/target/previous/value | 参数预检测试；需要 ValuePattern，真实应用分别验收 |
| [release-review-draft.yml](release-review-draft.yml) | 读取已打开的 actl Releases 页(Chrome),在 Word 新文档生成核对草稿并落盘 | 协议预检通过;读段与时效拒绝路径真机验证;写段联测待执行。验证记录见开发仓库 docs/43(技能包不含 docs) |

### release-review-draft.yml

- **必需信息**:无逐次输入;草稿文件名固定为 `actl-release-draft.docx`(Word 默认文档目录,同名覆盖,表示"当前核对状态"而非历史留痕)。
- **应用前提**:Chrome 已打开发布页且为当前标签(标题含 `Releases · gqy20/actl`);出现多个标题含 `Google Chrome` 的窗口时 AMBIGUOUS 拒绝,先收窄。本机无其他已打开的 Word 窗口。
- **授权边界**:读段(前两步)只读;写段从 `open_word` 检查点开始,需要显示端人工"开始"授权,一次授权跑完。`type --paste` 会覆盖系统剪贴板。
- **时效**:标题输出 `max_age_ms: 1800000`(30 分钟)。检查点后超时恢复以 `output_freshness_failed` 拒绝且不派发任何动作,须整跑重开。
- **写段待定案项**:Office 另存为页的文件名输入框按 `role:Edit` 定位、文件名框回车触发保存——联测时核对唯一性与语义,不符则收紧选择器;写段未验收前不宣称业务闭环。

“读取最近音乐 → 整理消息 → 定位好友 → 发送与核验”仍是候选验收场景;音乐窗口标题随曲目变化,
v1 无插值且公共文件禁固定 HWND,静态计划无法锚定,待 M3 契约扩展后再交付。仍需消息整理、
接收人消歧、数据时效核验及应用验证,不能将其视为现成可运行流程。

### 完整应用验证流程

| 流程 | 必需信息与前提 | 校验状态 |
|---|---|---|
| [vscode-edit.yml](vscode-edit.yml) | 独立 profile、两个任务文件、程序路径及现场选择器；file_result/second_result 限定 quickInput_list 内的任务文件名 | 预检及 Quick Input 组件探查通过；完整 22 步待复验 |
| [word-edit.yml](word-edit.yml) | 独立 DOCX、程序路径、正文选择器及每阶段预期文本 | 预检及合成文件断言通过；新流程真机待复验 |
| [excel-table.yml](excel-table.yml) | 独立 XLSX、程序路径、名称框及区域数据/公式预期 | 预检及合成文件断言通过；新流程真机待复验 |
| [zotero-search.yml](zotero-search.yml) | 独立 profile/data、RIS、导出路径及条目预期 | 预检通过；新流程真机待复验 |

上述四个流程及 vscode-resume.yml 的 inputs 已有运行上下文默认值，resources 声明隔离目录、初始文本和随包 Office 模板。直接运行：

```powershell
actl flow-run --file skills/actl/workflows/vscode-edit.yml --id vscode-001
```

每次从头运行使用新的 ID；无需先运行 PS 或生成 inputs.json。先只读预检可用 flow-validate --file 同一模板。可选 --inputs 仍支持覆盖默认输入；覆盖文件路径不会把资源写到该外部路径，调用方须自行提供并确认外部文件。

[verify-applications.yml](verify-applications.yml) 为四软件合计 64 步整批计划，资源按软件子目录隔离，整批共用一个任务 ID 和 once_per_run；其步骤与单软件模板的一致性由测试检查。首次交接及暂停恢复遵循当前授权规则。旧准备/合并脚本已删除，历史验收保留。

资源根目录为本机 actl 状态目录下的 flow-resources/<ID>。预检不创建资源；执行前完整预检通过才新建资源，已有同名目录拒绝覆盖。恢复不重建初始文件，也不重读模板。

这些完整验证流程会修改任务文件、剪贴板，并关闭核实归属的任务窗口。详细覆盖和限制见开发仓库 docs/57-application-workflow-migration.md；预检不代表真实软件验收通过。

## 2026-09-29 新迁移流程

以下模板已通过协议预检，真实软件写入复验待完成；此前脚本的实测结果不能直接沿用。
所有流程使用 once_per_run，同一任务恢复沿用原 ID。暂停、人工接管及结果不确定仍按引擎处理。

| 模板 | 必需输入/现场 | 核验 |
|---|---|---|
| [cloudmusic-playback](cloudmusic-playback.yml) | app=已观察的网易云 HWND；默认基线 play，若基线 pause，须同时交换 baseline/alternate 及对应 name= 谓词 | 4 轮 UIA/键盘切换并恢复，每一步核验控件名称；失败不盲目切回 |
| [quark-search](quark-search.yml) | app=已登录网盘页面的 HWND；query 为预期无结果的非空词 | 精确输入读回、解码 keyword、0 项与空结果文案、截图；不关闭已有窗口 |
| [tencent-meeting-draft](tencent-meeting-draft.yml) | app=已观察的 WeMeetApp 预定会议窗口；topic、expected_date | 主题与日期读回、截图；不提交会议 |
| [tencent-meeting-draft-date](tencent-meeting-draft-date.yml) | 同上，另给 date（yyyy/M/d） | 整组日期替换后核验，不把单段日期修改当成功 |
| [app-launch-observe](app-launch-observe.yml) | program=明确可执行文件，app=唯一标题；先确认无重复窗口 | 启动后等待窗口并观察；保留窗口。隔离编辑任务使用对应应用专用模板 |

输入文件是普通 JSON/YAML，例如 `{"app":"hwnd:123","topic":"测试草稿","expected_date":"2026/9/29"}`。
数字 HWND 仅示例，执行前重新观察并确认应用/进程/窗口类型。使用 flow-validate --file 模板 --inputs 输入预检，再 flow-run；无需额外 PS。
