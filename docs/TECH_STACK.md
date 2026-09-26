# TECH_STACK · 技术选型规格

版本:v0.1 · 日期:2026-09-26
定位:本文是**执行层规格**(用什么、怎么配);选型论证(为什么 Rust、为什么不是 C#/Go/Python)见 [04-tech-decision.md](04-tech-decision.md)。

---

## 1. 工具链

| 项 | 选型 | 说明 |
|---|---|---|
| 语言 | Rust **stable**(MSRV 见根 Cargo.toml `rust-version`) | edition 由 workspace 统一声明 |
| 目标三元组 | `x86_64-pc-windows-msvc`(唯一发布/开发目标) | 不使用 gnu 目标 |

## 2. Workspace 结构

```
computer_cli/
├── crates/
│   ├── actl-core/        # 协议:envelope、错误码、ref 引擎(纯逻辑,无 Windows 依赖,单测主场)
│   ├── actl-uia/         # Windows UIA 后端(windows-rs 封装:树遍历、pattern 分发、fallback 定位)
│   ├── actl-physical/    # 物理输入兜底(enigo,feature 可裁剪)
│   └── actl-cli/         # clap 入口、命令分发、进程/窗口/剪贴板/通知
├── fixtures/            # 集成测试场景(计算器/记事本/ms-settings 脚本)
├── docs/                # 本文档系列
└── .github/workflows/   # CI(windows-latest)
```

分层原则:`actl-core` 不 import 任何 Windows crate → 协议层可在任何平台单测/契约测试;`actl-uia` 是唯一接触 COM 的地方,脏活全部圈死在这一个 crate。

## 3. 依赖清单

| 用途 | crate | 版本策略 | 备注/备选 |
|---|---|---|---|
| UIA/COM | `windows`(微软官方) | **全 workspace 锁同一版本**(ADR-003);features: `Win32_UI_Accessibility`, `Win32_UI_WindowsAndMessaging`, `Win32_System_Com`, `Win32_System_Threading` | 唯一重度依赖 |
| UIA 高层封装 | `uiautomation`(**0.25,M0 评估期**:actl-uia 的 dev-dep,供 examples/probe 使用) | 评估结论(M1 采用或自写薄封装)写入 [spike-findings](spike-findings.md) |
| CLI | `clap`(derive) | 4.6 | 子命令即契约;help 文本与 `commands` schema 从 doc comment 同源生成(06 §3.5) |
| 序列化 | `serde` + `serde_json` | 跟 stable | envelope 唯一出口 |
| 错误 | `thiserror` | 跟 stable | 单一 Error 枚举 → 错误码映射(ADR-004) |
| 日志 | `tracing` + `tracing-subscriber` | 跟 stable | 仅写 **stderr**(ADR-002) |
| 物理输入 | `enigo` | feature `actl-physical` 内 | SendInput 级 |
| 进程/系统 | `sysinfo` | 评估 | 仅 launch/list 用;若够用则直接 windows-rs,少一个依赖 |
| 汇总 | — | 上限以 [AGENTS.md](../AGENTS.md) 硬规则 6 为准 | 本表是新增依赖的**唯一登记处** |

## 4. 架构决策记录(ADR)

| # | 决策 | 理由 |
|---|---|---|
| ADR-001 | **同步代码,不引入 async/tokio** | COM/UIA 是同步模型;CLI 生命周期毫秒级;async 只增加体积与复杂度 |
| ADR-002 | **stdout 只输出 JSON envelope,一切日志/诊断走 stderr**(例外条款:`--help/--version` 元命令走人类文本,见 docs/06 §3.5) | stdout 是机器契约,必须零污染;agent 与测试可放心管道 |
| ADR-003 | **`windows` crate 全依赖锁同一版本** | 该 crate 版本间类型不互通,是 Rust Windows 生态头号坑;用 workspace 统一版本 + CI 检查 |
| ADR-004 | **单一 `CtlError` 枚举,thiserror 派生,`code()` 映射错误码表** | 错误码是协议的一部分(PRD FR6),映射必须集中一处、可测试 |
| ADR-005 | **无状态:ref 不跨进程持久化,重定位仅在同 snapshot_id 内** | 契约简单严格优于模糊缓存;agent 重取成本已被 fallback 链 + 变化线索降到最低 |
| ADR-006 | **envelope `version` 字段与 CLI semver 解耦** | 协议演进慢于二进制;1.0 前允许 additive 变更,删改字段必须升 version |
| ADR-007 | **UIA 默认、物理显式(`--physical`)** | 安全默认值(PRD §4);feature `actl-physical` 可在编译期整体裁掉 |

## 5. 测试策略

| 层 | 工具 | 内容 |
|---|---|---|
| 单元 | 内联 `#[test]` | `actl-core` 全量:ref 编号/重定位、fallback 链选择、错误映射、谓词解析 |
| 契约 | 快照测试(insta 或手写 golden file) | 每个命令的 envelope 输出形状;schema 变更必须显式更新 golden |
| 集成 | fixtures 驱动(`cargo test --test integration`) | 真实驱动计算器/记事本/ms-settings 跑金任务的脚本化子集;CI 上跑,失败自动附 evidence |
| 性能 | 手动基准脚本 | 冷启动 100 次采样 / snapshot 耗时;M1、M2 退出时记录,不进常规 CI |

CI(GitHub Actions,`windows-latest`):`fmt --check` → `clippy -D warnings` → `test` →(main 分支)构建单 exe 上传 artifact。

## 6. 构建与分发

| 渠道 | 方式 | 阶段 |
|---|---|---|
| 源码 | `cargo install --git` / `cargo install actl` | M1 起 / M2 起(crates.io) |
| 二进制 | GitHub Releases:单 exe zip(<10MB,MSVC 静态 CRT `/crt-static`) | M2 起 |
| 包管理 | scoop manifest;winget 视社区需求 | M3 |

版本语义:0.x 期间允许破坏性变更(CHANGELOG 显式列出);1.0 后 semver 严格;envelope 变更走 ADR-006。

### Release 流程(tag 触发,release.yml 自动执行)

1. **手动部分**:整理 CHANGELOG(`Unreleased` → `## [vX.Y.Z] - 日期`,注意吸收合并历史条目、清理被取代的中间里程碑)、bump 根 Cargo.toml 的 `version`,一个 `chore(release)` 提交;
2. **先 push 再打 tag(顺序纪律,不许跳步)**:`git push origin main` → **等 CI 在该 commit 上全绿**(`gh run watch` 或 Actions 页确认)→ 才 `git tag vX.Y.Z && git push origin vX.Y.Z`。禁止 commit+tag 一步推:tag 指向未经 CI 验证的 commit,一旦红的就要删 tag 重打;release.yml 的 verify 段虽会重跑门禁,但那是最后防线而非流程依据;
3. tag 推送 → workflow 三路:**verify**(与 CI 完全同门禁)→ 并行 **release**(打包+冒烟+checksums+发布同 job 直传;仅真 tag 执行 gh release,从 CHANGELOG 提取对应版本段作 notes,缺段落回退自动生成)与 **crates**(仅真 tag:CARGO_REGISTRY_TOKEN secret 经 gh 配置;版本守卫 tag==Cargo.toml 后按依赖序 publish 三 crate)——GitHub Release 与 crates.io 由同一条 tag 同步驱动;发布产物同 job 直传 Release,不走 artifact 中转(artifact 存储是账号级共享配额,删除后 6-12h 才重算,曾连续阻断 v0.1.0 发版);artifact 归档保留为 best-effort 步骤(continue-on-error,配额不足自动跳过,永不阻塞),供演练下载核对;
4. 产物命名对齐 zotero_cli 体系:`actl_vX.Y.Z_windows_amd64.zip` + 裸 `actl_vX.Y.Z_windows_amd64.exe` + `checksums.txt`;
5. **不使用 UPX 压缩**:解压开销损害冷启动(违反 PRD 冷启动 P95 < 50ms 的 NFR);
6. 版本号单源于 Cargo.toml(`CARGO_PKG_VERSION` 编译期注入,无需 ldflags 类机制);commit/buildDate 注入 M2 需要时再加 build.rs;
7. workflow_dispatch 可手动演练(手动运行不发布,只出 artifact)。

## 7. 观察项(定期复审)

1. `uiautomation-rs` 维护活跃度 —— 断档则执行 §3 的弃用路径;
2. `windows` crate 大版本升级窗口 —— 每半年评估一次,升则全量;
3. agent-desktop 的协议演进(envelope 字段/错误码增补)—— 保持兼容压力测试:我们的 envelope 与其差异必须有登记表;
4. Rust edition/MSRV 跟进 —— 落后最多一个 stable 周期;
5. cargo-deny(依赖 license/漏洞审计)—— M2 依赖扩张后引入,当前直接依赖极少,过早;
6. MSRV 验证 CI job(`rust-version = 1.85` 目前只有声明无验证)—— 首个外部使用者出现前(M2)补。

## 8. 与既有文档的关系

- 选型**论证**(语言对比、冷启动数据、为什么不是 C#):[04-tech-decision.md](04-tech-decision.md)
- 命令面与协议**内容**: [06-design-blueprint.md](06-design-blueprint.md)
- 工期与行数:[05-implementation-estimate.md](05-implementation-estimate.md)
