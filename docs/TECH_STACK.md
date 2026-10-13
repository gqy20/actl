# TECH_STACK · 技术选型规格

版本:v0.2
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

actl-flow：YAML 协议、纯同步状态机与持久化，不依赖 Windows；CLI adapter 复用原子 dispatch。
serde-saphyr 与标准文件锁要求的 MSRV 已同步根 Cargo.toml；实际工具链仍只在 rust-toolchain.toml 锁定。
文件锁利用 OS 进程退出释放，不增加 unsafe、async 或后台服务。发布顺序增加 actl-flow → actl-cli。

## 3. 依赖清单

本表是新增依赖的**唯一登记处**(AGENTS 硬规则 6)。预算口径与授权边界:

- **核心执行路径**(core/uia/flow/cli/browser)直依赖 ≤9(actl-browser 现为 7:core/uia/serde/serde_json + chromiumoxide/tokio/futures——Phase 2 吸收后 tungstenite 退役,预算内;actl-cli 现为 8:core/uia/flow/browser + clap/serde/serde_json/ureq——ureq 为 decide 决策通道,2026-10-13 用户授权,预算内);边界适配 crate 的协议依赖单独登记,传递依赖不腐蚀核心。
- **用户已授权的预算例外**:①WebView2 显示层(+`webview2-com`,仅 actl-uia 详情窗口;TS/pnpm 仅构建期,发布嵌入本地静态页面);②MCP 适配 crate(+`rmcp`/`axum`/`tokio`/`getrandom`,**仅 actl-mcp**——不依赖任何 workspace crate,crate 边界编译器强制;协议层不手写=用户裁定:依赖数量不构成手写协议的理由)。
- **浏览器 CDP 通道**(用户已授权):`actl-browser` 全部动词承载于 `chromiumoxide` 骨干(Phase 2 终态吸收,2026-10-13:浅层手写客户端与 tungstenite 退役,双栈过渡态收束为单骨干);**命令面**守 cookie 誓言(负面 golden 钉死,docs/72 §8)。
- 前台交接 unsafe 限于:查询并持有执行进程句柄、核对同目录 actl.exe 路径与请求 PID、调用 AllowSetForegroundWindow(RAII 关闭,不用 ASFW_ANY、不注入 Alt、不改前台锁);动作租约核验显示端 PID/版本/租约并在动作退出时归还资格。
- 原型 `prototypes/browser`(Python/Playwright,uv 项目单独管理)不入 workspace/发布包。

| 用途 | crate | 版本策略 | 备注/备选 |
|---|---|---|---|
| UIA/COM | `windows`(微软官方) | **全 workspace 锁同一版本**(ADR-003);features: `Win32_UI_Accessibility`, `Win32_UI_WindowsAndMessaging`, `Win32_System_Com`, `Win32_System_Threading` | 唯一重度依赖 |
| 详情内容宿主 | `webview2-com` | =0.39.1 | 仅 actl-signal 的按需详情窗口；共享 Evergreen 运行时，失败回退原生内容；新增 1 个直接 Rust 依赖 |
| UIA 高层封装 | `uiautomation`(**0.25,M0 评估期**:actl-uia 的 dev-dep,供 examples/probe 使用) | 评估结论(M1 采用或自写薄封装)写入 [spike-findings](spike-findings.md) |
| CLI | `clap`(derive) | 4.6 | 子命令即契约;help 文本与 `commands` schema 从 doc comment 同源生成(06 §3.5) |
| 深水层 CDP | `chromiumoxide` | 跟 0.9 stable | 仅 actl-browser(bridge+ops/deep 全动词):类型化全协议域;async 圈在 bridge 同步门面后(ADR-001 修订);全树许可宽松(spike 实证,prototypes/browser-lib-probe);Phase 2 吸收后唯一 CDP 通道(手写 tungstenite 客户端已退役,docs/72 §5) |
| 深水层 runtime | `tokio` | 跟 1 stable(rt-multi-thread/time/sync 最小面) | 仅 actl-browser bridge:受控单 worker runtime,公共 API 保持同步 |
| 深水层 stream | `futures` | 跟 0.3 stable | 仅 actl-browser bridge:handler 泵(StreamExt) |
| MCP 协议 | `rmcp`(官方) | =3.5 | **仅 actl-mcp**(边界适配,ADR-001 修订)；server+streamable-http-server 最小 feature；协议层不手写(用户裁定,docs/82) |
| MCP HTTP 壳 | `axum` | 跟 0.8 stable | 仅 actl-mcp:承载 StreamableHttpService + Bearer/Origin 中间件 |
| MCP 熵源 | `getrandom` | 跟 stable | 仅 actl-mcp:256-bit 配对 token(纯安全 API 无 unsafe) |
| 决策 provider HTTP | `ureq` | =2(native-tls) | 仅 actl-cli decide:同步阻塞 HTTP,schannel 系统栈(不拖 rustls 树);System One 端点(Jev/Kev 族,docs/06 §7.9);命令面唯一出站网络触点,危险候选构造前剥离 |
| 序列化 | `serde` + `serde_json` | 跟 stable | envelope 唯一出口 |
| 流程 YAML | `serde-saphyr` | =1.3.0 | actl-flow；预算限制、重复键拒绝；不启用 include/robotics；实际外部直接依赖去重为 7（含 UIA 开发依赖） |
| ZIP/OOXML | `zip` | =8.2.0 | 仅 deflate-flate2-zlib-rs；运行时检查与开发打包共用 |
| 错误 | `thiserror` | 跟 stable | 单一 Error 枚举 → 错误码映射(ADR-004) |
| 日志 | `tracing` + `tracing-subscriber` | 跟 stable | 仅写 **stderr**(ADR-002) |
| 物理输入 | `enigo` | feature `actl-physical` 内 | SendInput 级 |
| 进程/系统 | `sysinfo` | 评估 | 仅 launch/list 用;若够用则直接 windows-rs,少一个依赖 |
| 汇总 | — | 上限以 [AGENTS.md](../AGENTS.md) 硬规则 6 为准 | 本表是新增依赖的**唯一登记处** |

OOXML 只读检查与发布归档共用 `zip`(禁默认特性,仅 deflate-flate2-zlib-rs);XML 走 windows crate 的 Windows.Data.Xml.Dom(禁 DTD/外部实体,深度 ≤128);进程查询用原生 Win32/WMI(COM/句柄 RAII 释放;WinRT 持一个进程生命周期 MTA 租约,与 windows-rs 缓存工厂生命周期一致避免缓存失效崩溃)。

## 4. 架构决策记录(ADR)

| # | 决策 | 理由 |
|---|---|---|
| ADR-001 | **桌面命令路径同步(core/uia/flow/cli/browser 的对外 API 与命令派发链不引入 async/tokio);async 仅允许出现在边界适配 crate(actl-mcp,crate 边界编译器强制)与 actl-browser 的 bridge 同步门面之后**(受控单 worker runtime,crate 对外 API 保持同步) | 原理由依然成立:COM/UIA 同步模型、CLI 毫秒级生命周期。修订护的是桌面命令路径的同步形态,不是给网络服务上锁;bridge 定界=async 不进 crate 公共 API/CLI 派发链;深水层动机与选型证据见 prototypes/browser-lib-probe(用户授权 2026-10-13) |
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

CI(GitHub Actions,`windows-latest`):`cargo xtask gate`(file-lines 单文件行数门禁 → pnpm 前端类型/样例/产物一致性 → `fmt --check` → `clippy -D warnings` → `test`)→(main 分支)构建单 exe 上传 artifact。前端检查需要 Node 24 与 package.json 指定的 pnpm；运行发布 EXE 不需要它们。行数门禁:crates/*/src 下 .rs 文件 >800 行警告、>1500 行失败(社区惯例手写代码 400–800 行/文件为宜;examples/tests 不扫,生成代码如引入须在 xtask 登记豁免)。

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
4. 完整 ZIP 包含 actl.exe、actl-signal.exe、skills/、README.md 和 LICENSE；打包统一调用 `cargo xtask package --version VERSION --binary-dir target/release --output dist`，压缩后校验技能入口、skills 文件内容及内部链接，门禁用隔离假二进制覆盖打包和缺文件拒绝。技能的运行说明集中在 `skills/actl/references/`，技能内不包含 URL，设计与验收文档留在开发仓库，不打包完整 docs。技能不会随解压自动注册，安装方式见 README。产物命名对齐 zotero_cli 体系:`actl_vX.Y.Z_windows_amd64.zip` + 裸 `actl_vX.Y.Z_windows_amd64.exe` + `checksums.txt`;
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

## 9. unsafe 豁免与实现边界登记

AGENTS 硬规则 5 的登记处。**共同不变量**(适用本节全部条目,不逐条重复):unsafe 仅存在于 `actl-uia`(含其 bin/signal 与 tests);COM/句柄/GDI 对象 RAII 释放,失败路径同样清理;回调最小化(只写调用栈内 Vec/原子计数,不持指针、不做回调内 IO)且低级 hook 必 CallNextHookEx;不注入输入、不激活窗口、不授予权限(诊断探针同规);除边界 crate actl-mcp 外零 async/常驻服务。

| 模块(actl-uia 内) | unsafe 范围 | 关键边界 |
|---|---|---|
| display_support / paint | 互斥锁探测、截图隐藏确认、窗口排除;Direct2D/DirectWrite/DXGI/WIC 绘制与预乘 BGRA 逐像素透明 | GDI 对象归还后释放,销毁停定时器 |
| display_gate | 显示端生命周期:CreateProcessW 禁句柄继承(GUI 子系统,避免常驻子进程持管道) | 不自动授予桌面权限 |
| signal(popup/controls/perimeter/shell) | SetWindowSubclass、DPI 字体、layered window、显示器枚举与四边像素条、--replace 进程同步+WM_CLOSE 有界等待 | 无全局输入 hook;每屏仅存窄边像素 |
| handoff(signal) | 非激活提示窗口、KBDLLHOOKSTRUCT/MSLLHOOKSTRUCT 低级 hook、有界 SendMessageTimeout 查询 | 回调只递增原子序号;RAII 卸载;grant 只在显示进程内存 |
| foreground | SetForegroundWindow 单次请求+核验实际前台,AllowSetForegroundWindow 归还资格 | 不注入 Alt、不放宽前台锁;已在前台不重复请求 |
| preview_window | 自有预览窗口创建/销毁、消息派发、只读窗口检查、绘制 | 不注入输入、不持桌面授权 |
| evidence | WMI/进程句柄、ZIP/OOXML、Windows.Data.Xml.Dom | XML 禁 DTD/外部实体,深度 ≤128 |
| find_image | WIC PNG **解码**(模板→BGRA,与截图编码同栈零新依赖)、PrintWindow 窗口捕获、GetWindowRect 原点 | 匹配算法在 actl-core(纯逻辑无 unsafe);64M 像素上限;均匀模板/越界 PROTOCOL |
| native_windows / observation | EnumWindows 同步回调、FindAllBuildCache/TreeWalker 直调、数组 Length/GetElement | 不跨调用缓存元素;负长度拒绝为 INTERNAL;枚举错误传播不伪装空列表 |
| 输入监听(Raw Input) | 注册、消息窗口、计时器、GetRawInputData 对齐缓冲读取 | 校验长度/来源/类型后读;不存键值坐标;不可识别事件保守计外部输入 |
| history 清理 | std 文件锁,60 分钟限频,显示端后台线程 | 前台资格转交与审计不在 UI 回调扫描历史 |

诊断探针(tests,默认 ignore 不进 CI):provider_probe(MTA/COM 客户端创建与只读查询、可比较 CUIAutomation8 与句柄缓存)、桌面根身份探针(≤16 直接子元素缓存身份,不读标题)、逐节点 TreeWalker 探针(≤256 项,DPI 对照仅临时切线程 PMv2 并恢复)——均不设超时、不调动作模式、不激活窗口,结果只存阶段/身份/错误。观察观察层(observation_query 等)与生产共用同一 MTA 客户端,不跨调用缓存,不以缓存当授权或长期身份依据。

actl-core 侧(零 unsafe):activity/signal_session/state 用 std 线程+channel;操作历史与任务报告 std 文件 IO+serde_json,每调用独立 JSONL,写入处清理。CLI:build.rs 为 actl 预留 4 MiB 栈(clap 命令树超 MSVC 默认 1 MiB);devtools 由 xtask 转发,gate 单点在 xtask;CLI 输出重定向独立文件,超时只结束本次启动的 CLI 子进程,不据超时推断动作未送达。
