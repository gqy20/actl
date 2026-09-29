# 原生工具迁移与旧脚本清理

日期：2026-09-29。此前更改已提交为 7b677a3、5284b86、a4cc477；本文件记录后续迁移。

## 入口

业务操作直接使用 `actl flow-run --file skills/actl/workflows/名称.yml --inputs values.json --id 唯一ID`。
恢复使用原 ID 的 `actl flow-resume --id ID`。不再运行启动/暂停包装脚本。

开发工具使用 Rust 实现，入口如下；输出为 actl-core JSON envelope，进度进 stderr。

| 目的 | 入口 |
|---|---|
| 查看回归清单 | `cargo xtask regress --list` |
| 无人值守的只读恢复测试 | `cargo xtask regress --suite flow --unattended` |
| 单个系统夹具 | `cargo xtask fixture input-target`（或 golden-task-1/2、virtualized-list、win32-table-extract、edge-deep-tree、edge-content-view） |
| Chrome 场景夹具 | `cargo xtask fixture chrome-content-view` |
| 显示端反馈 | `cargo xtask fixture signal-display --hwnd HWND --physical`；调用方明确提供测试窗口，保留窗口 |
| 显示就绪与 scoped prepare/restore | `cargo xtask fixture display-readiness --hwnd HWND`；失败保留现场及恢复 session，不强制收尾 |
| 观察已有软件 | `cargo xtask diagnose app-observe --hwnd HWND` |
| 冷调用与投影采样 | `cargo xtask diagnose phase2-readonly --samples 100 [--hwnd HWND]` |
| 局部观察复验 | `cargo xtask diagnose local-observation --hwnd HWND --within SELECTOR` |
| 复合选择器 | `cargo xtask diagnose uia-composite-observe --hwnd HWND --automation-id ID` |
| 外部 UIA 对照 | `cargo xtask diagnose uia-compare --hwnd HWND --winapp ABSOLUTE_EXE` |
| 发布归档 | `cargo xtask package --version vX.Y.Z --binary-dir target/release --output dist` |

诊断/夹具默认使用 target/validation/release/actl.exe，可用 --actl 指定被测二进制，--output 必须指向新的证据目录。
regress 会先构建验证二进制；fixture/diagnose 可独立指定已构建版本。所有结果留 target，不写回历史验收记录。

## 41 个 PowerShell 文件的去向

| 旧实现 | 新实现或明确退役的行为 |
|---|---|
| file_inspect.ps1（1） | actl-uia/evidence.rs：进程句柄、WMI、OOXML；运行时不启动解释器 |
| scenarios/app-observe.ps1（1） | diagnose app-observe 只观察明确 HWND；app-launch-observe.yml 负责显式启动观察。删除按窗口差集猜归属并自动关闭的做法 |
| scenarios/cloudmusic-playback、quark-search、tencent-meeting-draft（3） | 同名 workflow；会议日期写入单列 tencent-meeting-draft-date.yml；保留精确读回与查询参数解码核验 |
| scenarios/phase2-readonly、local-observation、uia-compare 及各自 lib（6） | diagnostics.rs、checks.rs、capture.rs；截断/变化/错误不算削减验收通过 |
| scenarios/uia-composite-observe（1） | diagnose uia-composite-observe |
| scenarios/chrome-content-view（1） | fixture chrome-content-view |
| fixtures/system-fixture、system-scenarios、golden-task-1、win32-table-extract、virtualized-list（5） | fixtures.rs 与 capture/checks，进程实例、任务标记及窗口身份检查；关闭只发一次，再核验实例退出 |
| fixtures/golden-task-2、input-target、input-target-lib（3） | Notepad 原生夹具，专用文件、绑定输入目标、焦点拒绝、旧 ref 拒绝、保存及任务标签清理 |
| fixtures/browser-probe、browser-probe-lib、edge-deep-tree、edge-content-view（4） | 浏览器原生夹具，隔离 profile/页面、动态内容稳定、歧义与输出体积断言 |
| fixtures/flow-evaluation（1） | 原生夹具：冻结计划、检查点、真实 CLI 暂停/排他/进程退出与只读恢复 |
| fixtures/signal-display、display-readiness（2） | 原生显式 HWND 夹具；可见租约/历史停止代次分别由 core/display、stop 与信号生命周期测试覆盖。删除写/删旧急停文件、强制关闭窗口的旧测试做法；完整显示端端到端验收仍待复验 |
| xtask/regress、regress-lib、package-release（3） | actl-devtools regress/package；xtask 只转发，gate 仍唯一维护在 xtask |
| xtask/tests 下 10 个 PS 测试（10） | Rust 的 capture、checks、regress、package、evidence、flow_contract 测试及原有关闭/焦点/显示契约测试。原先 mock 旧 PS 函数和扫描包装脚本文本的测试随实现退役 |

合计：scenarios 12、fixtures 15、xtask 13、运行时 1，共 41。仓库不再保留 .ps1。

## Bash 清理的范围

删除 6 个仅转发 PS 的入口：golden-task-1/2、win32-table-extract、virtualized-list、edge-deep-tree、edge-content-view。
另退役 11 个旧实验：us1-cross-app、coord-click、timing-profile、timing-sweep、bili-deep5、douyin-vision-loop、ext-deep3、xhs-browse、xhs-browse-chrome、xhs-deep2、xhs-login-check。
它们包含硬编码现场、标题清理、旧 ref、输出混流或无核验重试，不能作为可靠回归入口。源码保存在 Git 历史；历史结果不改写。
跨应用搬运、坐标输入和动态网站的替代端到端验收仍在 ROADMAP，删除脚本不表示这些实验已迁移通过。

## 验证与限制

- native-migration-tests.log：原生证据读取、哈希连续调用、ZIP 包内容/链接/缺文件、子进程双输出捕获、超时及后代句柄、状态选择、窗口归属和局部采样测试。
- migration-flow-contract.log：21 个流程契约测试通过，包含 5 个新模板预检及 URL 参数契约。预检不操作桌面。
- native-flow-evaluation.json：真实选定 CLI 完成只读流程恢复夹具；被测 SHA-256 为 819730e4b01c8c730a02b7497eccbee0107eaffd911e2dfaba8430e7bf29d9d1。测试只结束自己创建的 CLI，不结束 GUI。
- native-local-sampler.log：初次失败，迁移错误地要求 list-windows 必有 visible=true；该协议省略默认值。
- native-local-sampler-retest.log：修复并补测试后，自建窗口的全局/局部/重复局部/全局采样、snapshot 绑定 get 与清理通过，未抢前台。它不替代 ZCode/VS Code 整轮实测。
- hash-repeat-test.log：保留连续 WinRT 调用崩溃的复现；修复进程生命周期 MTA 租约后，native-migration-tests.log 的重复哈希和连续 OOXML 读取通过。
- native-migration-final-gate.log：最终 cargo xtask gate 通过，包含文件行数、格式、clippy、workspace 测试和 11 个 pack/296 条命令校验。
- native-release-package.json：真实发布二进制归档及 38 个包成员校验通过。
- native-release-regress.json：cargo xtask regress --suite flow --unattended 发布版入口通过；不启动应用写入。
- 本机 cargo/bin 及已登记 live-runtime 的 CLI 已更新，SHA-256 为 C152E37129D8CB49DF9806DB94B54817603940ED47C470CF4EF132716A143DE1，大小 5,195,776 字节；安装路径的 DOCX 核验通过。替换前无运行中的 actl/actl-signal，未自动启动显示端或续跑旧任务。
- 真实软件写入、桌面 prepare/restore 与新 workflow 整轮验收仍待完成。

原生 WMI 连接没有原 PS 子进程的 20 秒硬超时。枚举 Next 有 5 秒上限，文件和 XML 有容量/深度上限；不宣称整个断言硬实时。
诊断度量是 UTF-8 字节，不是 tokenizer token；必须在同环境头、同二进制和稳定现场下比较。
