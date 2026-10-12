# 40 · 自动化测试与评估系统(调研与设计提案)

状态:**提案,未实施**。整理自 2026-09-28 设计调研,含同日两轮修订:初版以"专用主机
+ Hyper-V"为主线,修订一确认本机为 Windows 11 家庭版并澄清 Hyper-V 三层语义,修订二
据"去掉强启"目标将虚拟化后端改为可插拔(VMware 主线)。若与 AGENTS/docs 现行规则
冲突,以现行规则为准。

## 1. 问题定义

现有体系已有离线门禁(CI 跑 `cargo xtask gate`)与真机回归入口(`cargo xtask
regress`,需交互桌面、人工盯着跑)。缺口不在"有没有测试",而在三件事:

1. **执行环境**:regress 跑在开发机,不满足"键盘注入 fixture 需桌面空闲",失败后
   桌面状态不确定,只能人工复核;
2. **可复位基线**:评估(同环境 N 次重跑统计成功率/flake)在定义上要求环境可复位,
   裸机做不到;
3. **指标层**:report.json 有逐用例证据,但没有聚合指标(成功率、flake 率、错误码
   分布),ROADMAP "US1 十次重跑成功率 ≥90%" 缺直接验收物。

## 2. 执行环境选型

| 方案 | 结论 | 依据 |
|---|---|---|
| Docker 容器直跑 | **永久排除** | 四条独立死因:Windows 容器无交互桌面;Server 镜像里**没有**计算器/记事本/ms-settings(无物可测);actl.exe 为 MSVC Windows 二进制,Linux 容器无法执行;CRIU checkpoint 仅 Linux 侧 |
| GitHub 托管 runner | **保留于 L0**;L1 观察类子集值得一次探针 | runner 跑在 session 0,物理注入不可靠;纯 UIA 只读可能可行,需实测 |
| 自托管 runner | 可行但最后一步可选 | 公开仓库 fork PR 可在解锁桌面会话获得代码执行;需 fork 审批 + label 门控;推荐先用 pull 模式 |
| **专用主机 + 可插拔虚拟化后端 + VM** | **推荐主线** | 交互桌面 + 快照秒级复位 + 爆炸半径受控;后端选择见 §6 |

## 3. 四层架构(映射现有双轨纪律)

```
L0 离线门禁        windows-latest + xtask gate        (已有,不动)  push/PR,分钟级
L1 真机 fixture 轨  专用VM + 夜间无人值守 regress       (本提案)    每夜+手动,快照兜底
L2 场景实测轨      真机+真应用+人监督,docs 记录         (维持现状)  设计使然,不进 CI
L3 评估系统        同VM同基线,N次重跑+指标报告          (本提案)    支撑 ROADMAP 90% 验收
```

L2 保持人工是刻意的:真实应用登录态、云同步、外部依赖的噪声本身就是被测对象。

## 4. GUI 评测集调研

| 评测集 | 适配度 | 用法 |
|---|---|---|
| Windows Agent Arena(WAA) | ★★★ | **借语料不借框架**:任务 initializer/verifier 移植,环境清单参考;MIT |
| OSWorld | ★★ | harness 设计蓝本(snapshot→init→run→verify 循环);任务在 Ubuntu 不可复用 |
| OfficeBench | ★☆ | 任务形态与 scenarios/ 撞型,参考任务设计与评分法 |
| ScreenSpot-Pro | ★ | 仅 vision 观察轨相关(静态 grounding);UIA 主线无关 |

关键认知:这些评测集测"LLM agent 的大脑",actl 是"手和眼"。对 actl 的价值不在跑分,
而在解决两件贵事:**带 verifier 的任务语料**(机器可查的"做对了没有")与**标准化环境
清单**。注意 WAA VM 以 LibreOffice 等免费应用为主,无正版 MS Office,替代不了场景轨。

## 5. Agent 跑分可行性

让 Claude Code(或同类)配 skills/actl 跑 WAA 语料:**可行,有标准先例**。链路:
harness(宿主)restore/init/verify,VM 内 headless Claude Code 读 skills/actl 调
actl.exe。actl stdout 严格 JSON envelope 对 agent 消费友好。

三个工程决策点:

1. **执行位置**:actl 必须 run 在被控 VM 交互会话内;宿主↔VM 下发/回收通道即
   ROADMAP 中 MCP 适配层的形态——跑分管道与 MCP 计划是同一件事两个视角;
2. **成本**:154 任务 × 几十步 × 多模态上下文,单轮全量数小时到数十小时、数十到数百
   美元;观察优先用 UIA 树 JSON 控 token,截图兜底;
3. **失败归因分层**(比总分更有价值):LLM 决策错 / actl 命令失败(按 ErrorCode 分桶)/
   **护栏拒绝(by design,单独列,不算缺陷)** / 环境与超时。产出的差距清单直接喂
   ROADMAP 与 skills/actl 引导文本改进。

## 6. 虚拟化后端选型(本机为家庭版)

**本机探测结果**(2026-09-28):Windows 11 家庭版中文版;底层 hypervisor 已在跑
(`HypervisorPresent: True`,WSL2 使用中),Hyper-V 管理角色未启用(vmms 缺席),但
`servicing\Packages` 内有 178 个 Hyper-V 组件包——能力在,版本策略不开开关。

"Hyper-V"三层语义须区分:底层 hypervisor(家庭版有)/ Virtual Machine Platform
与 Windows Hypervisor Platform(家庭版开放)/ Hyper-V 完整管理角色(官方仅 Pro+)。
强启脚本(DISM 从 servicing\Packages 装包)在家庭版多数可用,但属不受支持配置,大版本
更新后可能失效需重跑。

**关键解耦原则:宿主 hypervisor 的选择不影响 guest 内部发生的事**——UIA、actl、
fixture、agent 全在 guest 里。hypervisor 只决定管理面(快照/通道/性能)。因此 guest
基线可移植,换后端只重写管理脚本层。

候选与优先序(目标:零不受支持配置):

| 优先 | 方案 | 支持状态 | 能力对位 |
|---|---|---|---|
| 1 | **VMware Workstation Pro** | 官方支持家庭版;2024 年底起个人使用免费 | `revertToSnapshot`↔checkpoint 复位;链接克隆↔差分盘多 lane;`vmrun runScriptInGuest`/`copyFileFromGuestToHost`↔PowerShell Direct(免网络通道) |
| 2 | WSL2 嵌套虚拟化 + QEMU(可套 Docker) | 全官方链(`nestedVirtualization=true` 文档化) | QEMU/qcow2 快照;SSH 通道;**可直跑 WAA 官方 harness**;三层嵌套排障链路长 |
| 3 | 原生 QEMU + WHP 加速 | Home 官方开放的可选功能(`-accel whp`) | 官方但生态孤僻,WAA 不直接用 |
| 4 | 升 Pro / 换 Pro 机 | 官方 | 一步回到 Hyper-V 原生管理面,长期服役主机可考虑 |

注意事项:本机 WSL2 已占用底层 hypervisor,VMware 以 WHP 共存模式运行,性能损失
几个点到十几个点——对计算器/记事本级负载无关痛痛;三条路线不互斥,guest 基线通用,
Packer 模板多声明一个 builder 即可切换。强启 Hyper-V 从主线降级为记录在案的备选,
若采用则 host-doctor 必须加"角色在位"检查。

## 7. 专用工具与生态调研

- **WinAppDriver 已事实性停维护**(GitHub issue #1103),继任 Appium Windows Driver
  依赖它且脆弱。战略含义:actl 占据的正是这个生态位,且官方方案已死;工程含义:
  任何一层都不得搭在 WinAppDriver/Appium 上;
- **云端空白**:Sauce/BrowserStack/LambdaTest 均为 web/mobile,无原生 Windows 桌面
  GUI 网格;E2B 等 agent 沙箱为 Linux-only(Firecracker 结构限制)。"云端 checkpoint
  即服务"暂不存在,一年后值得复查;
- **商业整包**(UiPath Test Suite、TestComplete、Ranorex、Eggplant)方向相反:它们
  用自家驱动测"你的应用",无法用于测 actl 本身,仅证明市场存在;
- **Packer(引入)**:HCL2 模板 + autounattend.xml,同一模板声明 vmware/hyperv/qemu
  多 builder。`vm-build.ps1` 的装机逻辑改为声明式 Packer 模板,只装宿主、只在重建
  基线时运行,不进运行路径依赖;
- **Vagrant / Ansible(不引入)**:单主机单 VM 规模下 xtask + 幂等 pwsh 已覆盖,
  引入只增加依赖与心智模型,与仓库"单点实现、依赖最小化"文化相悖。

## 8. 主机与 provisioning

**本机磁盘与位置规划**(探测:C: 剩 46 GB 紧张,D: 122 GB,E: 344 GB,G: 446 GB;
OneDrive 在 USERPROFILE 下):

| 内容 | 位置 | 理由 |
|---|---|---|
| VM 与镜像资产 | `E:\vm\`(vm/disks/iso 或 VMware 默认位置重定向至此) | 剩余充足;与仓库同盘;系统盘重装不影响 |
| 仓库 | `E:\explore\computer_cli`(现位置) | 短路径、无空格无中文(GBK 纪律) |
| Rust 工具链 | 默认 `~\.cargo/.rustup` 可用;介意 C: 空间则 `RUSTUP_HOME`/`CARGO_HOME` 重定向 `E:\rust\` | 工具链约 3–5 GB,C: 仅剩 46 GB |
| 评估历史(JSONL 趋势/跨 run 聚合) | `E:\actl-eval\`(仓库外) | `cargo clean` 会清 target/,历史不能放里面 |
| 明确不放 | OneDrive 目录、桌面/文档 | 同步锁文件 + target/ 巨量垃圾同步 |

G: 空间最多但盘类型未确认:使用前 `Get-PhysicalDisk | ft FriendlyName,MediaType`,
机械盘或外接盘不做 VM 盘(快照复位达不到秒级 + 掉盘风险)。

**安装序列**(宿主,管理员):
1. VMware Workstation Pro(官方渠道,个人免费授权;或后续底座对应的等价物);
2. `winget install Git.Git / Microsoft.PowerShell / Rustlang.Rustup / Hashicorp.Packer`;
3. 仓库内 `cargo xtask setup`(hooks + pinned 工具链);
4. ISO 下载至 `E:\vm\iso\`,建 VM 交给 Packer 模板 + `vm-build.ps1`。

**provisioning 三层分离**(不是追求 100% 无人化):幂等脚本(Hyper-V/电源/
auto-logon/装依赖/建 VM/打快照/计划任务);半自动(ISO 条款、部分应用安装);必须人工
(激活、凭据、API key 等 secrets 不进仓库;场景轨登录态只能用户点击)。落位沿用
xtask 家族:`host-setup.ps1` / `vm-build.ps1`(调 Packer)/ `host-doctor.ps1` +
`cargo xtask host setup|doctor|snapshot`。脚本不写版本号(唯一来源原则)。
**先手工走通再固化脚本**。

`host-doctor` 产环境 identity 报告(工具链/OS build/快照基线 ID/应用清单/hypervisor
在位性),作为 eval report header 字段兼做漂移检测;能从脚本重建同一环境,"固定环境
头"才成立。

## 9. 数据、日志与评分设计

数据流水线(envelope 是原子,各层已有大半):

```
eval → run(report.json,已有) → case(stdout/stderr/capture,已有)
     → step(flow state.json,已有;agent 轨迹,待补) → call(envelope + ErrorCode,已有)
```

补三样:关联 ID 贯通(evalId/基线 ID 进 header);agent 决策轨迹归档;保留策略
(机器产物留本地 target/ 不进 git,docs 不写机器本地路径)。

评分用**记分卡,不是单一分数**,两层永不混算:

- 原语稳定性分(确定性 flow,同基线重跑 N 次):调用成功率(按命令族)、
  **flake 率**(跨次不一致比例)、恢复成功率、耗时 p50/p95(同环境头内)、错误码分布;
- agent 任务分(WAA 式 verifier):任务成功率、失败归因分解、平均步数、每任务成本。

原则:状态分类沿用 passed/failed/blocked/skipped,受阻不折算;护栏拒绝单列不算缺陷。
汇总产物 JSON + 自动生成 markdown 记分卡 + JSONL 追加历史,不上数据库/仪表盘。

文档反馈环:机器报告(本地)→ docs/ 评估记录(环境 identity/记分卡/限制与存疑,
原失败记录不改写)→ ROADMAP(待办)→ pack/skill(agent 用错模式则修引导文本)。
触发规则:新 flake → 待办 + 先写复现测试;错误码新聚类 → 同上;发版前 → 全量 eval
作验收证据。

## 10. 代码量与工期预估

校准基准:regress 全套 126 行 pwsh 驱动 17 用例;xtask 全部 Rust 380 行;单 fixture
109–170 行。行数门禁 WARN 800/DENY 1500,新模块余量充足。

| 组件 | 语言 | 行数 |
|---|---|---|
| 评估数据层(report 扩展 + xtask eval 聚合器 + 看门狗) | Rust/pwsh | 450–750 |
| 主机 provisioning 三脚本 + 接线(装机逻辑=Packer 模板声明,净变化约 ±0) | pwsh/HCL/Rust | 500–900 |
| WAA 语料移植(10–20 任务胶水) | pwsh/JSON | 300–800 |
| Agent 实验(harness + 归因分类器 + skill 微调) | pwsh | 350–700 |
| 测试(Pester + Rust 单测) | — | 150–300 |
| 文档 | md | 200–400 |
| **合计** | | **约 1,600–3,100,中位 ~2,300** |

分阶段:Phase 0 ~50–100 行(一天内);Phase 1 ~500–800 行(三到五个工作日);
Phase 2 ~1,000–2,000 行(两到三周)。**代码量不是成本大头,真机调试时间是**;
浮动最大的是 WAA 移植量与 agent harness 厚度。

## 11. VM 的作用与资源需求

五个作用:失败后秒级确定性复位(替代"桌面不确定需人工复核");评估统计可比性
(同基线定义);版本钉死(数字可归因于代码而非环境漂移);爆炸半径控制(agent 会在
桌面犯蠢,VM 撞坏就还原);并行 lane 与可重建。VM 不提高保真度——`--physical` 与
真应用验收留裸机。

资源(被测对象是系统应用,属轻负载):单 VM 4–6 GB(agent 轨 8 GB);宿主 16 GB 紧凑、
32 GB 从容(仅跑评估时占用);单 VHDX ~30 GB,快照为差分/增量语义,稳态 +几 GB;
256 GB SSD 从容、512 GB 可开并行 lane。省资源:链接克隆/差分盘做多 lane;exe 宿主
构建后拷入,VM 免装工具链。注意:快照放 SSD;耗时指标用固定内存 VM(动态内存引入
抖动,与 timing 纪律相性差)。

## 12. 落地路线

1. **Phase 0(一天,零新基建,先做)**:宿主机上手动装 Claude Code + 配 skills/actl,
   自主完成 2–3 个金任务,验证"大脑+手"链路与 skill 引导质量;同时加 GitHub runner
   session 0 纯 UIA 探针 workflow(半小时);
2. **Phase 1**:eval 最小闭环——report schema 扩展(evalId/基线 ID)、`xtask eval`
   聚合器、看门狗、夜间计划任务;VM 侧:VMware + Packer 模板出基线镜像;
3. **Phase 2**:provisioning 全家(xtask host)、WAA 语料移植、agent harness 与归因。

## 13. 红线(全流程适用)

- 看门狗/provisioning **永远不检测即清除急停标记**,只能停机报警(AGENTS 现行规则);
- secrets(凭据/API key)不进仓库不进脚本明文,doctor 只校验在位不打印;
- L2 场景轨不进无人值守 CI;WAA VM 无正版 Office,Office 真版任务留场景轨;
- 耗时数字仍只同环境头内比较(专用主机降低现场方差,纪律不变;慢现场先排自身污染——AGENTS 失败处置协议);
- 评估记录原失败不改写,受阻不记通过;
- 任何一层不得依赖 WinAppDriver/Appium 生态(已事实停维护);
- guest 基线与宿主 hypervisor 解耦:换后端只动管理脚本层,不动 guest 与评估体系。

## 14. 参考资料

- Windows Agent Arena:[arXiv:2409.08264](https://arxiv.org/abs/2409.08264) ·
  [项目页](https://microsoft.github.io/WindowsAgentArena/) ·
  [GitHub](https://github.com/microsoft/WindowsAgentArena)(MIT)
- OSWorld:[arXiv:2404.07972](https://arxiv.org/abs/2404.07972)
- GitHub 托管 runner GUI 限制:[community #36704](https://github.com/orgs/community/discussions/36704)
- Windows 容器无交互桌面:[GitLab runner docs](https://gitlab.com/gitlab-org/gitlab-runner/-/blob/main/docs/install/windows.md) ·
  [WinAppDriver #1046](https://github.com/microsoft/WinAppDriver/issues/1046)
- WinAppDriver 停维护:[issue #1103](https://github.com/microsoft/WinAppDriver/issues/1103)
- 自托管 runner 安全:[GitHub Secure use](https://docs.github.com/en/actions/reference/security/secure-use) ·
  [Praetorian 案例](https://www.praetorian.com/blog/tensorflow-supply-chain-compromise-via-self-hosted-runner-attack)
- ScreenSpot-Pro:[HF 数据集](https://huggingface.co/datasets/likaixin/ScreenSpot-Pro)
- Packer Windows 镜像实践:[progressiverobot.com](https://www.progressiverobot.com)
- E2B 沙箱(Linux-only):[e2b-dev/desktop](https://github.com/e2b-dev/desktop) ·
  [Firecracker vs QEMU](https://e2b.dev)
