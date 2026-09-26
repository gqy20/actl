# Changelog

本项目的全部显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),
版本语义遵循 SemVer;0.x 期间破坏性变更在条目中以 **[BC]** 标注。

## [Unreleased]

### Added
- 命名决策:项目定名 **actl**(agent control);bin `actl`、crate `actl-cli`(冲突查证见 docs/06 §1)。
- 命令面设计定稿:扁平动词式 + 命名四规则 + `commands` 自发现命令 + L1/L2/L3 分层模型与解耦/整合原则(docs/06 §3)。
- 帮助系统规范:`-h` 三级层级、stdout 协议例外条款(--help/--version)、help 英文与同源生成(docs/06 §3.5)。
- **M1 CLI 骨架**:envelope 输出管线(计时/退出码 0-1-2)、`status` 与 `commands`(自发现 schema,clap 同源生成)、bin 集成契约测试;clap 4.6 入依赖(TOTAL_DIRECT_DEPS=5/8)。
- **M1 snapshot**:`actl snapshot [--app X] [--skeleton]` —— UIA 树遍历(actl-uia,`uiautomation` 升正式依赖)、`@eN` ref 编号与 skeleton 祖先链投影(actl-core 纯逻辑 + 单测)、NOT_FOUND 错误契约;计算器实测 204ms(debug 构建)。
- **M1 交互链路**:`click`(UIInvoke 后台调用,支持 `@eN`/`name:`/`id:`/`role:` 四种目标)、`get`(属性白名单读取含 ValuePattern)、`launch`(spawn + trailing args);PROTOCOL 错误改走 exit 2(对齐 06 §4);真实链路验证:launch calc → click num7Button(显示"七")→ get CalculatorResults 读出"显示为 77"。
- **金任务 1 达成 10/10**(fixtures/golden-task-1.sh):计算器 123×456+789 → 56,877,120 次点击零失败,ref 重放(snapshot→@eN→click)100% 稳定;性能实测 P50 40.2ms / P95 53.9ms / snapshot 275ms / 二进制 0.78MB。
- **[spec] PRD 冷启动指标按实测修订**:P95 50ms → 80ms(附加 P50 < 50ms 约束)——Windows 进程 spawn 固定开销 ~30ms 为不可压缩项。
- **spec(错误码)**:新增 `INTERNAL`(内部/UIA/系统层错误);envelope 增加可选 `snapshot_id` 字段;actl-core 枚举落齐 `ASSERTION_FAILED`。
- 命令面语义审计(docs/06 §3.6):补 `key-up`、`inspect` 并入 `get --raw`、`clipboard-*` 更名 `get-clipboard`/`set-clipboard`(对齐命名规则)、新增错误码 `ASSERTION_FAILED` 并定义 `--expect` 等待语义;近似命令对(type/set-value、wait/verify、snapshot/find 等)的分工裁定入档。
- `cargo xtask` 任务入口(setup/gate/fmt):门禁实现收敛为 xtask 单点,pre-push 与 CI 均调用 `cargo xtask gate`;Makefile 作为惯例入口保留(纯转发层)。
- tag 驱动的 release 流水线(verify→build→publish,对齐 zotero_cli 体系;流程文档 docs/TECH_STACK §6)。
- pre-commit hook 增加 merge-conflict 标记检查(等价 pre-commit-hooks 官方集)。

### Changed
- **[BC]** 全部 crate 由 `ctl-*` 更名为 `actl-*`,bin 名 `actl`。

### 仓库规范基建
- workspace 骨架(actl-core / actl-uia / actl-physical / actl-cli)、git hooks(Conventional Commits 校验 + pre-push 测试)、CI(windows-latest)、AGENTS.md 协作规范。
- `actl-core`:envelope v1 与错误码枚举骨架(协议定义见 docs/06-design-blueprint.md)。
- 文档体系:PRD / ROADMAP / TECH_STACK / 调研系列 docs/01–06;单一事实来源审计完成。
