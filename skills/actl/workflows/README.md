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
| `scenarios/` | 真实应用验收脚本和辅助代码 |
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
- 当前 v1 支持只读步骤的显式输出保存和后续完整参数绑定；绑定无法解析时停止，不执行占位值。不支持参数插值、条件分支、循环或子流程调用。任务输入通过 inputs 声明及 --inputs 文件传入，args 中使用 {input: name} 绑定完整参数；解析后统一预检，恢复读取保存的具体计划。
- 应用档案供 agent 参考，当前引擎不支持 YAML 自动引用 pack 路线。未来加入复用能力前须先修订协议、测试和文档。
- 对会过期的业务输出设置 `max_age_ms`。恢复报告 `output_freshness_failed` 时，先核对已完成动作，再准备后续计划；不要修改保存时间或盲目从头重跑。时间未过期仍需 before/verify 核验目标身份和业务状态。
- 需要恢复前核对当前页面、会话或接收人时声明 `resume_check`；首次执行条件仍放 before。只读目标失效可设 `retry: stale_read`，最多重新查询三次。`result_observation_failed` 表示结果尚不明确，不能通过 --retry-step 绕过；先解决观察问题。用法见 [流程恢复说明](../references/flows.md#暂停与恢复)。

## 流程索引

| 流程 | 用途 | 校验状态 |
|---|---|---|
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
| [vscode-edit.yml](vscode-edit.yml) | 独立 profile、两个任务文件、程序路径及现场选择器 | 预检通过；新流程真机待复验 |
| [word-edit.yml](word-edit.yml) | 独立 DOCX、程序路径、正文选择器及每阶段预期文本 | 预检及合成文件断言通过；新流程真机待复验 |
| [excel-table.yml](excel-table.yml) | 独立 XLSX、程序路径、名称框及区域数据/公式预期 | 预检及合成文件断言通过；新流程真机待复验 |
| [zotero-search.yml](zotero-search.yml) | 独立 profile/data、RIS、导出路径及条目预期 | 预检通过；新流程真机待复验 |

全部输入在各文件的 inputs 中声明。开发仓库 scenarios/prepare-workflow.ps1 可生成隔离资源及 inputs.json；通过 -Scenario 选择四个流程，桌面动作由 YAML 执行。scenarios/verify-applications.ps1 将四份已解析计划合并为一个 flow，整批使用同一任务 ID 和 once_per_run。首次仍需用户点击开始；暂停恢复显示继续，并先核验现场。旧同名入口及对照脚本已删除；历史验收结论保留在文档和 Git 历史。

这些完整验证流程会修改任务文件、剪贴板，并关闭核实归属的任务窗口。详细覆盖和限制见开发仓库 docs/57-application-workflow-migration.md；预检不代表真实软件验收通过。
