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
顶层可选:`display` · `verified_env`(验证环境,如 `Win11 24H2 zh-cn`)·
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

## 模板

```yaml
pack_format: 1
name: myapp
display: 我的应用
launch: MyApp            # 进程名;裸名走 App Paths 解析
window: "我的应用"        # 窗口标题子串
app_version: "1.0"
verified_date: 2026-01-01
verified_env: Win11 zh-cn   # 可选
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
