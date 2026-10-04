# 录制草稿的 agent 改写轮

录制（`record-*` 命令）产出**机械草稿**：能 flow-validate、每步带 `# evidence: trace:<行号>`
证据引用，但目标选择、等待与验证都是保守的机械形态。改写轮把"可跑"提升为"可复用健壮"——
这是 agent 的工作，不是引擎内置能力（LLM 不进 CLI）。验收判据：**改写后重放通过率 ≥ 机械草稿**。

## 输入与产物

会话目录 `%LOCALAPPDATA%\actl\records\<id>\`：

| 文件 | 用途 |
|---|---|
| `draft.yml` | 机械草稿（改写起点，不要直接编辑它） |
| `trace.jsonl` | 证据文件（每行一个事件；TODO 项的行号指到这里） |
| `rewritten.yml` | **改写产物约定位置**（本地文件，个人任务值不进技能目录） |
| `state.json` | 会话状态（events/steps/todos 计数） |

## 改写步骤

1. **先读草稿的 TODO 注释**（draft.yml 尾部 `# - [trace:N] …`）——每条 TODO 自带
   trace 行号；按行号定点读 `trace.jsonl` 对应事件，不整文件进上下文（trace 是本地
   证据，不是 token 载荷）。
2. **逐类修复**（TODO 类型 → 做法）：
   - *unresolved click*（反查失败）：用 trace 里的窗口标题 + 坐标现场重新观察——
     `find --name … --app <标题>` 或 `snapshot --app <标题>`，从真实控件取
     `id:`/`name:` 选择器替换；不猜。
   - *terminator keys*（enter/tab/esc/F 键）：按业务语义补验证——如提交后断言
     新窗口出现（`verify --window` 语义走 `wait --window` 或 verify）或目标文本变化；
     表达式在 TODO 注释里。
   - *IME 序列*（无值读回）：现场 `get --value <目标> --app <标题>` 读当前值，
     或人工提供文本，落成 `type <目标> <文本> --paste --app <标题>`。
   - *password 字段*：保留占位步骤、标注人工填写；**密码内容永不写进任何 YAML/trace**。
   - *non-left click*：确认语义后映射为 `right-click`/`double-click`。
3. **稳健化既有步骤**：trace 事件携带元素全身份（automation_id/name/runtime_id）。
   稳定性序与 pack 相同：`id:` > `name:`（显示文本，语言/状态可变）> 位置。机械选择器
   已按此序，改写时核对 `name:` 是否含动态内容（计数、用户名）。
4. **等待补强**：机械 wait 只覆盖"换窗+间隔"形态；同窗但内容加载（列表填充、
   路由切换）补 `wait --element … --property value=<期望>` 或 `--stable`。
5. **噪音清除**：对照 trace 识别"点错又点回"（连续两个相反操作），合并为最终意图
   步骤；trace 里的注入事件本来就不编译，无需处理。
6. **后置条件最小化**（发现 #8 教训）：动作后的窗口断言只断言该动作真正保证的
   事实——"enter 启动应用"只保证「应用窗口出现」，不保证「标题=演示当时所见」
   （标题可能依赖标签会话状态，跨次重放会漂移）。标题级断言若依赖会话巧合，
   降级为窗口类/进程级断言（如 `wait --window Notepad`），或在前加显式步骤把
   状态确定化（如 ctrl+O 打开具体文件）。
7. **输入通道先验证再采用**（发现 #7 教训）：剪贴板粘贴不是万能文本通道——
   搜索框类控件（启动器/过滤框）常只认真实击键流，粘贴只建标签不驱动筛选。
   改写为 `--paste` 前先确认该控件吃粘贴（pack 有结论优先读 pack）；无结论的
   搜索框默认疑粘贴死路，逐键 `press` 序列起步。逆序亦然：逐键太慢时换通道
   前同样先验证。
8. **参数化**：判断步骤字面值是数据还是常量。数据的提升为 `inputs`
   （如文件名→ `inputs: {filename: {default: …}}` + `{input: filename}`）；
   `--app` 锚已是 `{input: windowN}` 形态，直接复用。
9. **敏感标注**：改写后步骤含删除/发送/支付类语义（词法闸同族判断）加
   `pause_before: true`。
10. **校验**：`actl flow-validate --file records/<id>/rewritten.yml`——不过不跑。

## 重放对比验收

1. 准备与录制开始前等价的应用状态（pack 的 reset 路线或人工复位）。
2. 基线：`actl flow-run --file <draft.yml> --id rw-base-<id>`（可 `--paused` 先核对计划）。
3. 再次复位后：`actl flow-run --file <rewritten.yml> --id rw-after-<id>`。
4. 比较 `flow-status` 的步骤 verified 数与失败步骤；**通过 = rewritten ≥ base**。
   失败步骤如实保留（不重写记录），按错误码 evidence 自愈后可整轮重跑。

注意：重放执行真实写入，授权/暂停/急停语义与普通 flow 相同；写前确认目标应用
是隔离测试数据；一轮重放解决多个问题（重编译前先列全本轮假设）。

## 复核迁移（可选终点）

改写后的路线若要沉淀为应用知识：按 PACKS.md 纪律，路线片段进 `apps/<slug>.yml`
（status 恒 unverified），人工复核后迁移 verified。技能目录不含个人任务值；
录制草稿与改写产物都留在本地 records/ 目录。
