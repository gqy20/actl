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

新增 actl-flow：YAML 协议、纯同步状态机与持久化，不依赖 Windows；CLI adapter 复用原子 dispatch。
serde-saphyr 与标准文件锁要求的 MSRV 已同步根 Cargo.toml；实际工具链仍只在 rust-toolchain.toml 锁定。
文件锁利用 OS 进程退出释放，不增加 unsafe、async 或后台服务。发布顺序增加 actl-flow → actl-cli。

## 3. 依赖清单

2026-09-28 浏览器接入原型：`prototypes/browser` 使用 Python 同步 Playwright 1.54.0，
以 uv 项目单独管理。复用本机已安装环境进行验证，不加入 Rust workspace/发布包；Rust 外部直接依赖仍为 7，
另登记 1 个可选原型直接依赖。Python/浏览器为原型运行前提，无新增 Rust async/tokio 或常驻服务。

| 用途 | crate | 版本策略 | 备注/备选 |
|---|---|---|---|
| UIA/COM | `windows`(微软官方) | **全 workspace 锁同一版本**(ADR-003);features: `Win32_UI_Accessibility`, `Win32_UI_WindowsAndMessaging`, `Win32_System_Com`, `Win32_System_Threading` | 唯一重度依赖 |
| UIA 高层封装 | `uiautomation`(**0.25,M0 评估期**:actl-uia 的 dev-dep,供 examples/probe 使用) | 评估结论(M1 采用或自写薄封装)写入 [spike-findings](spike-findings.md) |
| CLI | `clap`(derive) | 4.6 | 子命令即契约;help 文本与 `commands` schema 从 doc comment 同源生成(06 §3.5) |
| 序列化 | `serde` + `serde_json` | 跟 stable | envelope 唯一出口 |
| 流程 YAML | `serde-saphyr` | =1.3.0 | actl-flow；预算限制、重复键拒绝；不启用 include/robotics；实际外部直接依赖去重为 7（含 UIA 开发依赖） |
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

CI(GitHub Actions,`windows-latest`):`cargo xtask gate`(file-lines 单文件行数门禁 → `fmt --check` → `clippy -D warnings` → `test`)→(main 分支)构建单 exe 上传 artifact。行数门禁:crates/*/src 下 .rs 文件 >800 行警告、>1500 行失败(社区惯例手写代码 400–800 行/文件为宜;examples/tests 不扫,生成代码如引入须在 xtask 登记豁免)。

## 6. 构建与分发

| 渠道 | 方式 | 阶段 |
|---|---|---|
| 源码 | `cargo install --git` / `cargo install actl` | M1 起 / M2 起(crates.io) |
| 二进制 | GitHub Releases:单 exe zip(<10MB,MSVC 静态 CRT `/crt-static`) | M2 起 |
| 包管理 | scoop manifest;winget 视社区需求 | M3 |

版本语义:0.x 期间允许破坏性变更(CHANGELOG 显式列出);1.0 后 semver 严格;envelope 变更走 ADR-006。

### Release 流程(tag 触发,release.yml 自动执行)

1. **手动部分**:整理 CHANGELOG(`Unreleased` → `## [X.Y.Z] - 日期`,注意吸收合并历史条目、清理被取代的中间里程碑)、bump 根 Cargo.toml 的 `version`,一个 `chore(release)` 提交;
2. **先 push 再打 tag(顺序纪律,不许跳步)**:`git push origin main` → **等 CI 在该 commit 上全绿**(`gh run watch` 或 Actions 页确认)→ 才 `git tag vX.Y.Z && git push origin vX.Y.Z`。禁止 commit+tag 一步推:tag 指向未经 CI 验证的 commit,一旦红的就要删 tag 重打;release.yml 的 verify 段虽会重跑门禁,但那是最后防线而非流程依据;
3. tag 推送 → workflow 三路:**verify**(与 CI 完全同门禁)→ 并行 **release**(打包+冒烟+checksums+发布同 job 直传;仅真 tag 执行 gh release,从 CHANGELOG 提取对应版本段作 notes,缺段落回退自动生成)与 **crates**(仅真 tag:CARGO_REGISTRY_TOKEN secret 经 gh 配置;版本守卫 tag==Cargo.toml 后按依赖序 publish 四 crate（core → uia → flow → cli）)——GitHub Release 与 crates.io 由同一条 tag 同步驱动;发布产物同 job 直传 Release,不走 artifact 中转(artifact 存储是账号级共享配额,删除后 6-12h 才重算,曾连续阻断 v0.1.0 发版);artifact 归档保留为 best-effort 步骤(continue-on-error,配额不足自动跳过,永不阻塞),供演练下载核对;
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
7. 显示层渲染栈(GDI → Direct2D/WebView2 内容层)—— 触发条件:docs/11 Phase B/C 面板化立项,或信号条视觉迭代摩擦不可接受;路线矩阵与决策门见 [11 §14](11-desktop-interaction-and-display-research.md);若采 webview2-com 须先登记 §3 依赖清单(+1~2 个直接依赖)。

## 8. 与既有文档的关系

- 选型**论证**(语言对比、冷启动数据、为什么不是 C#):[04-tech-decision.md](04-tech-decision.md)
- 命令面与协议**内容**: [06-design-blueprint.md](06-design-blueprint.md)
- 工期与行数:[05-implementation-estimate.md](05-implementation-estimate.md)


## 显示层 v2 实现登记（2026-09-27）

零新增直接依赖。`actl-core::{activity,signal_session,state}` 保持纯 Rust，心跳使用
标准线程与 channel，不引入 async/tokio。`actl-signal` 拆为 model/paint/overlay/shell。
unsafe 仍仅在 actl-uia：新增 display_support 用于互斥锁探测、截图隐藏确认和窗口排除；
feedback 的物理焦点读取及原生显示模块的 GDI/消息泵/透明窗口操作沿用该 crate 豁免。
所有 GDI 对象归还后释放，标记窗口销毁时停止定时器；无全局输入 hook。

状态条精细绘制复用 windows crate 的 Direct2D / DirectWrite / Dxgi Common feature，
不新增直接依赖。WIC 软件渲染输出预乘 BGRA，UpdateLayeredWindow 提交逐像素透明度；
COM 与 DIB/DC 生命周期仅位于 actl-uia 的 paint 模块，原生调用需要该 crate 的 unsafe 豁免。

显示就绪协议 v3 仍使用文件租约与 std 轮询，不新增依赖。desktop 模块使用 UIA 身份和 Win32
窗口状态 API 保存/恢复布局；signal/icon 生成各原生尺寸的预乘 BGRA 图标，不引入外部资源代码。
CLI 增加命令后，clap 调试构建超过 MSVC 默认 1 MiB 栈保留；build.rs 为 actl 预留 4 MiB，
页面按需提交。已有 CLI 启动/契约测试覆盖此前栈溢出复现。
显示端为 Windows GUI 子系统；自动启动使用 CreateProcessW 禁止句柄继承，避免常驻子进程持有
调用者管道而令 PowerShell 等待。该原生启动只位于 actl-uia/display_gate，按既有 unsafe 豁免执行。

操作历史与任务报告使用 actl-core 的 std 文件 IO + 已有 serde_json，零新依赖、零 unsafe。
每调用独立 JSONL，避免跨进程共享追加流；写入处清理，不增加后台服务。
存储失败用 stderr 诊断；显式归档命令返回统一错误 envelope。保留与容量边界见 docs/06 §7.6。

本轮显示补充仍复用现有 Win32 依赖：popup 的 SetWindowSubclass 保留静态文本原生可访问角色，
仅跳过重复绘制；controls 的字体随 DPI 更新并在销毁时释放。shell --replace 使用进程同步句柄、
WM_CLOSE 和有界等待完成正常替换；句柄在成功/失败路径均释放。以上 unsafe 仅位于 actl-uia。

屏幕边缘提示复用 actl-uia 的原生 layered window 与 paint 的预乘 Alpha 提交函数，无新增依赖。
perimeter 内 unsafe 用于显示器枚举、各屏 DPI、窗口生命周期与绘制；枚举回调只同步写入调用栈内 Vec，
窗口失败时清理已创建部分，隐藏/退出/截图均销毁。每屏仅存四条窄边像素，不分配全屏动画缓冲。

窗口基线复用现有 windows 依赖：`actl-uia/native_windows` 的 unsafe 仅用于 EnumWindows
同步回调及读取 HWND 的原生属性。回调 LPARAM 指向调用栈中存活的 Vec，不保留指针，
不取得窗口所有权、不发送消息、不更改前台。原生枚举错误向上传播，不伪装为空列表。


桌面交接复用现有 std + serde_json + windows，无新增依赖。纯授权逻辑位于 actl-core/handoff。
actl-uia/bin/signal/handoff 的 unsafe 用于原生非激活提示窗口和低级输入 hook：回调仅读取
系统在回调期提供的 KBDLLHOOKSTRUCT/MSLLHOOKSTRUCT，递增原子序号并始终 CallNextHookEx，
不持有指针、不做回调内 IO、不屏蔽输入；RAII 卸载 hooks。客户端仅用有界 SendMessageTimeout
查询，不跨进程传递指针。window_ready 的 unsafe 仅诊断/恢复/核验目标 HWND，不绕过前台限制。
显示 v5 强制新旧两端兼容；grant 只在显示进程内存，磁盘请求不是授权。
