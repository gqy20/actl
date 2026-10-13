# 应用档案 pack 契约(pack_format: 1)

pack 是"给 agent 读的应用知识"——路线(routes)+ 怪癖(quirks)+ 信任元数据,
由 skill 层消费;**核心 CLI 永不读 pack**(应用知识不进核心,见 AGENTS.md 仓库地图)。
本文件是 pack 格式的单一事实来源;机械校验 = `cargo xtask lint-packs [目录]`
(已入 gate,pre-push 与 CI 随之生效)。

## 定位与边界

- `apps/` 是应用知识；完整业务执行计划放在同一 actl 技能的 [workflows/](workflows/README.md)。
  两种 YAML 使用不同契约；当前引擎不自动展开 pack 路线，也不支持子流程引用。

- pack 不是可盲跑的脚本:执行永远走 actl 自身护栏(`--app` fail-closed、前台核对、
  Ctrl+Alt+F12 急停)。高危路线必须自带核对步骤(见红线)。
- pack 与 docs/ 场景战记的分工:战记是**过程证据**(怎么踩的坑、环境如何),pack 是
  **沉淀后的结论**(路线与解法)。深度过程进 docs/(双轨纪律),PR 轻量记录见下。

## 文件与命名

- 位置 `skills/actl/apps/<slug>.yml`;一应用一文件,slug 用小写英文
  (wechat.yml,不用中文名/空格/大写)。
- 顶部 `#` 注释头记录知识来源与验证纪要(样例见 wechat.yml)。
- 应用大版本不兼容(如微信 4.x → 5.0 界面体系重构):新建 `<slug>vN.yml`
  (如 wechat5.yml),旧文件保留服务旧版用户,勿在一个文件里混写两代。

## 字段(pack_format: 1)

顶层必填:`pack_format`(固定 `1`)· `name` · `launch` · `window` ·
`app_version`(适配的应用版本范围,如 `"4.1.x"`)· `verified_date`(YYYY-MM-DD,
全 pack 最近一次系统验证)· `routes`。
顶层可选:`display` · `verified_env`(验证环境,规范形态 `Win11 25H2 (build 26200) zh-cn`——营销名+build 号。**勿写内核 10.0 前缀**(Win11 永远报 10.0,无判别力)也勿信注册表 ProductName(微软遗留字段,Win11 仍写 "Windows 10");build 号才是行为判别器)·
`verify`(全 pack 通用的核对方法)。

`quirks[]`(可选,应用怪癖):`id`(snake_case,全文件唯一)+ `note`;
可带 `unblock` 步骤数组(解法即步骤,同样过命令校验)。

`routes[]`(必填,至少一条):

- `id`(snake_case,全文件唯一)、`description`、`steps` 必填;
- `status` 必填,三档:
  - `verified`:真机走通(日期 = verified_date,环境记入 verified_env);
  - `manual`:人工手工路径,未自动化验证;
  - `unverified`:推测/未测——agent 不得直接执行,仅供人读;
- `verify`(建议):该路线的核对方法;**高危路线(发送/删除/支付等不可逆动作)
  必填**,且建议升到 `verified` 后才交给 agent 执行。

## 占位符

`{window}` 固定指顶层 `window` 值;`{text}`/`{name}`/`{contact}` 等其余变量由
调用方注入。lint 会把 `{...}` 替换为哑值后校验步骤首 token 必须是真实 actl 命令
(命令全集取自 `actl commands`,单一事实来源,防 CLI 演进后 pack 漂移)。
注意:`screenshot --element` 的窗口作用域参数是 `--sapp`(`--app` 被"截整窗"占用,
锚点同族为 `--snear`)。

## 观察结果与变量绑定

- **选择器形态优先级（2026-10-06 性能轮约定）**：元素有稳定 AutomationId 时步骤优先 `id:`——`find --id` 单一精确条件走 provider 侧单次跨进程调用（快路径），`name:/role:` 走全树扫描（子串语义）。`name:` 保留给无 id 的元素（Electron/CEF 系普遍无 automation id）；lint-packs 对 `name:` 步骤给 advisory 提示。
- `{...}` 由调用方根据任务和现场观察填写，不是引擎自动插值。`apps/` 不能直接交给 `flow-run`；持久化流程须使用自己的协议和稳定语义选择器。
- `*_ref` 一律填完整的 `@eN`，不再手工追加 `@`。对应 `*_snapshot_id` 必须取自生成该 ref 的同次 snapshot/find 响应；不得取另一次观察的快照 ID。
- 每次 find 后核对名称、角色、窗口与候选唯一性，再填该步骤之后使用的变量。再次 find 同一字段时更新整对变量；写入或界面变化后旧身份失效，先重新观察，不自动重放写入。
- `*_selector`、`anchor`、`target` 默认表示现场核对唯一的语义选择器（如组合定位），不得偷偷填入未绑定 ref。不支持 `--snapshot` 的命令也须先重新观察目标。
- 输入框的 Enter、全选、删除、粘贴优先使用 `press --target`；ref 目标同时绑定 `--snapshot`。它只断言焦点，不会聚焦；被拒绝时先核对实际输入框，不删除参数绕过拒绝。
- `verify` 是调用方必须执行的结果核验要求，不能把读取成功当成内容匹配。窗口清单、空闲查询和 UIA 观察失败均保留错误；不能转成“没有窗口”“没有人操作”或“已关闭”。

## 修订后的验证状态

修改实际步骤、定位或输入通道后，原 `verified` 路线改记 `manual`，由调用方逐步观察复验；原 `unverified` 保持不变。未修改的路线可保留历史状态。
`verified_date` 保留历史真机日期，不能以静态检查日期刷新。注释说明本次修订与待复验状态，原场景证据不覆盖；完整新版复验后再按实际结果更新状态和日期。

## 探索回写与隔离协议（2026-10-12 定型）

**双轨分工,不统一**:引擎轨(`explore-writeback`)从 goal_met 的探索运行抽取路线片段(status 恒 unverified),调用方合并进 pack;**自由探索轨(agent 直接驱动 CLI 原语)的回写就是 agent 手工编辑 pack**——quirks 给 agent 读、stats/priors 给引擎用,两种知识形态不同,不互相格式化,不建自动写入通道(verified 迁移=人工复核+PR,红线)。

**模型应答不改变回写纪律**(2026-10-13):探索决策卡可经 `actl decide` 模型辅助应答
(危险候选构造前剥离、低置信弃权交回人)——运行可能无人工卡裁决即达 goal_met;
回写 status **仍恒 unverified**,人工复核与 PR 红线不因此移动。

**隔离策略前置声明**:探索开工前先读 pack 的隔离声明,不现场试错。quirk 形式登记(不新增结构字段):

```yaml
  - id: exploration_isolation
    note: 隔离策略:single-instance 应用 -datadir/-profile 会转发给既有实例,隔离实例开不出来——直接走真实实例+测试条目协议;写验证已在真实库与独立库双轨完成。
```

已知形态:`datadir 可隔离`(记可用的参数形态,如反斜杠要求)/`profile 可隔离`/`single-instance 隔离不可行`(直接声明走真实实例协议)。

**真实实例探索协议**(隔离不可行时,须用户明确授权;2026-10-12 Zotero 轮全程实证):
1. 产物(测试条目/测试文件)用可识别标记名,动作前后各一次 `find` 核对出现与消失;
2. 写入走最稳通道(`type --verify-value`),产物核验走 `verify-file --discover`(过滤器补扩展名陷阱);
3. 清理走应用内白名单动词(删除确认框按 pack quirk 定位),完成后 `find` 空返回核实;
4. 收尾 `residue-report` 全量核对崩溃框残留,与任务前基线做差集;
5. 清理失败如实报,不静默放弃测试产物。

## 模板

```yaml
pack_format: 1
name: myapp
display: 我的应用
launch: MyApp            # 进程名;裸名走 App Paths 解析
window: "我的应用"        # 窗口标题子串
app_version: "1.0"
verified_date: 2026-01-01
verified_env: Win11 25H2 (build 26200) zh-cn   # 可选
# verify: 全 pack 通用核对方法(可选)
quirks:
  - id: some_quirk
    note: 怪癖与解法,实测证据写这里
    # unblock: ["click", "name:启用编辑", "--app", "{window}"]
routes:
  - id: open_and_read
    description: 打开并读取(一句话:做什么 + 关键怪癖提示)
    status: verified
    # verify: 读回 X 与预期比对
    steps:
      - ["snapshot", "--app", "{window}", "--skeleton"]
```

## 贡献流程(公开应用,进本仓库)

1. 复制模板建 `apps/<slug>.yml`,注释头写知识来源;
2. `cargo xtask lint-packs` 本地过(gate 也会跑);
3. PR 内容 = pack + 轻量实测记录(关键 envelope 输出、踩坑两三行即可;
   深度实测记录保存在开发仓库，应用档案只保留经验证的结论);
4. review 清单(红线见下)通过即合并;贡献者即该 pack 的维护责任人。

### review 红线(违反即拒)

- **盲发/无核对的高危路线**:发送、删除、支付等不可逆动作后没有读回/终验步骤;
- **规避护栏**:绕过 `--app`、绕过前台核对、任何隐身/规避执行的写法;
- **与核心语义冲突**:steps 里出现非 actl 命令(lint 会拦)或参数凭空发明(review 拦)。

## 私有/企业适配(不进本仓库)

内部应用的 pack 放自己的仓库(quirks 与路线可能涉密),同格式、同校验:

```
cargo xtask lint-packs path/to/your/packs
```

需要本仓库的 Rust 工具链;校验逻辑在 crates/xtask `lint_packs`(零依赖),可自行移植。

## 生命周期

- `verified_date` 超半年未复验的路线,消费侧应视同 `manual`;
- 用户报 STALE_REF/路线失效 → issue 给该 pack 责任人 → 复验刷新 `verified_date`,
  或降级 `status`;
- 格式演进:`pack_format` 递增,lint 按版本分支;旧格式设过渡期后拒绝。
