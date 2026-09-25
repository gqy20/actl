# AGENTS.md — 协作规范(人与 AI 共用)

操作规则的单一事实来源;背景与理由见 docs/,不在此重复。改动本仓库前必读本文。

## 硬规则(违反 = 拒绝,编号对应 docs/TECH_STACK.md 的 ADR)

1. **stdout 只输出 actl-core 的 JSON envelope**,日志/诊断只进 stderr(ADR-002;唯一例外 `--help/--version` 走人类文本,见 docs/06 §3.5);
2. **`windows` crate 只在根 Cargo.toml 声明一次**,全 workspace 同版本(ADR-003);
3. **失败必须有错误码**:统一走 actl-core 的 Error → ErrorCode 映射,运行路径禁 `unwrap/expect`(ADR-004);
4. **不引入 async/tokio**(ADR-001);
5. **`actl-core` 禁止依赖 windows 相关 crate;`unsafe` 只允许出现在 `actl-uia`**(workspace 已 deny,仅该 crate 豁免并登记理由);
6. **直接依赖总数 ≤ 8**,新增先登记 docs/TECH_STACK.md §3;
7. **默认 UIA,物理输入必须显式 `--physical`**(ADR-007);
8. **协议/命令语义变更(spec 提交)必须同 PR 完成**:契约 golden 测试 + docs/06 + CHANGELOG,三者同步。

## 工作流

- trunk-based:`main` 始终绿;短生命周期分支 `feat/xxx`、`fix/xxx`,合并即删;
- 提交:Conventional Commits `<type>(<scope>): <subject>`,首行 ≤100 字符(hook 校验;type 全集以 `hooks/commit-msg` 正则为最终权威,本列表须与之同步);
  type: `feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert|spec`(spec = 协议变更,触发硬规则 8);scope: `core|uia|physical|cli|protocol|release`;破坏性加 `!`;
- **门禁单点**:`pre-push` hook 与 CI 都执行 `cargo xtask gate`;门禁命令的**唯一实现在 `crates/xtask`**,改门禁只改那一处;
- 跳过门禁仅限紧急:`git push --no-verify`,且在 PR 说明。

## 环境

- 工具链锁死于 [rust-toolchain.toml](rust-toolchain.toml)(版本唯一来源,含 msvc target),不要绕过、不要在他处写版本号;
- clone 后:`make setup`(惯例入口,等价 `cargo xtask setup`;一步装 hooks + 激活 pinned 工具链);提交前:`make gate`;
- **Makefile 只是转发层**:全部目标转发到 `cargo xtask ...`,门禁的唯一实现单点在 `crates/xtask`;禁止在 Makefile 写实现逻辑;
- 无 make 的环境(如纯净 Windows)直接用 `cargo xtask setup / gate / fmt`。

## 仓库地图(分层细节见 docs/TECH_STACK.md §2)

`crates/actl-core`=协议(纯逻辑) · `crates/actl-uia`=UIA/COM(唯一 unsafe 豁免) · `crates/actl-physical`=物理兜底 · `crates/actl-cli`=入口 · `fixtures/`=集成测试 · `hooks/`=git hooks · `docs/`=全部决策文档

## 测试

- 新命令:单测 + envelope 契约 golden 最低配;fixtures 只用系统自带应用(计算器/记事本/ms-settings);
- 修 bug:**先写复现测试再修**,同 PR;
- golden 变更只能随 `spec`/`test` 提交,禁止顺手改。

## AI agent 附加要求

- 交付前跑 `cargo xtask gate`,如实报告结果(含失败);
- 命令/协议语义以 docs/06-design-blueprint.md 为准,不凭记忆发明;
- 代码与文档同 PR;范围以 docs/PRD.md §1.3 非目标清单为铁律,拿不准就问,不静默扩大;
- 安全:不做任何规避权限/隐身执行的功能;涉及 `--physical` 的改动必须显式提示;
- 提交信息自写、按逻辑分组,禁巨型混合提交。

License: TBD(倾向 Apache-2.0);定案前不引入 vendored 外部代码。
