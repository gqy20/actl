# 05 · 实现估算:基于同类仓库实测校准

> 历史调研/估算，保留当时口径。当前范围以 [PRD](PRD.md) 和 [ROADMAP](ROADMAP.md) 为准：
> MCP 计划在被控 Windows 设备本地启动供其他设备调用；Linux/macOS 执行后端暂不考虑实现。
> 下文对纯 CLI 的估算不包含未来 MCP 服务。

日期:2026-09-26 · 方法:shallow clone 三个同类开源仓库,统计 Rust 代码行数(`find -name "*.rs" | xargs cat | wc -l`,已修正分批统计误差)

## 1. 实测数据

### terminator(99,252 行 / 202 文件,Windows UIA)

| 组成 | 行数 | 对我们的意义 |
|---|---|---|
| 核心 UIA 库(`crates/terminator`) | 34,018 | Windows UIA 脏活的"全深度"参照 |
| MCP 层(`terminator-mcp-agent`) | 39,189 | **不要 MCP ⇒ 直接省掉四成工程量** |
| workflow 录制器 | 11,735 | 不做 |
| CLI(`terminator-cli`) | 5,640 | CLI 层的规模参照 |
| TS SDK(packages)+ computer-use | 8,670 | 不做 |

### agent-desktop(125,784 行 / 1023 文件,macOS)

| 组成 | 行数 | 对我们的意义 |
|---|---|---|
| 协议+命令层(`crates/core`) | 60,686 | 含测试/契约/多版本 envelope,**协议层并不便宜** |
| macOS 平台层 | 38,808 | 单平台后端的"认真深度"参照 |
| FFI 多语言绑定 | 14,920 | 不做 |
| CLI 入口(src) | 10,839 | |
| **windows / linux crate** | **76 / 76** | **空壳,确证无现成代码可复用** |

### desktop-cli(15,039 行 / 90 文件,单人三平台)

Windows UIA 层仅 ~2,500 行,协议+语义+CLI ~7,000 行——证明"激进下限"存在,代价是覆盖深度。

## 2. 两个关键发现

1. **MCP 是最重的非必要负担**:terminator 的 MCP 层(39k)> 核心库(34k)。"纯 CLI"决策直接省四成。
2. **协议层比直觉贵**:agent-desktop 的 core 达 60k 行(含测试与契约设施)。初版估算曾按 5k 协议层计,据此上调——认真 MVP 的协议+引擎层按 8–15k 计。

## 3. MVP 模块估算(Rust,对齐 agent-desktop 命令面)

| 模块 | 行数估算 | 依据 |
|---|---|---|
| CLI 定义与分发(clap,~45 命令) | 1,500–2,500 | terminator-cli 5,640 打对折 |
| JSON envelope + 错误码契约 | 600–1,000 | |
| ref/snapshot 引擎(DFS 编号、重定位、STALE_REF、fallback 定位链、skeleton 投影) | 2,500–4,500 | agent-desktop core 的对应子集 + RPA 借鉴项 |
| UIA 后端(树遍历、属性映射、pattern 分发、过滤) | 3,000–6,000 | terminator 34k 的纯净子集;desktop-cli 2.5k 为下限 |
| 交互命令(UIA pattern 优先 + SendInput 兜底) | 1,500–3,000 | |
| 窗口/应用/进程/剪贴板/wait/batch/verify/extract | 1,500–2,500 | desktop-cli 对应模块 1.3k + RPA 借鉴 |
| 集成测试(计算器/记事本 fixtures) | 1,000–2,500 | |
| **合计** | **约 12,000–22,000** | 中位 ~16k |

## 4. 三档规模与工期(solo 开发)

| 档位 | 规模 | 工期 | 说明 |
|---|---|---|---|
| 激进 MVP | 5,000–8,000 行 | 2–4 周 | ~20 核心命令、主流应用可操作、契约从简;desktop-cli 作者验证过该下限 |
| 认真 MVP | 12,000–22,000 行 | 1.5–3 个月(AI 辅助编程可压至 4–6 周) | 45+ 命令、完整错误契约、STALE_REF 重试、skeleton 投影、verify/extract |
| 生产级 | 30,000–50,000+ 行 | 长期积累 | 长尾应用 workaround、浏览器/Canvas 兜底、滚动等待策略——靠 Issue 驱动,非一次写成 |

## 5. 工期黑洞警告

行数分布与直觉相反:**协议与命令层薄而愉快(~6k),UIA 后端才是行数黑洞(5–8k)且调试时间约为编写时间的 3 倍**(每个不配合的应用都是一次 COM 侦查)。预算时按"写 1 行调 3 行"给 UIA 层留时间。

不熟 UIA 的 Rust 开发者:整体 ×1.5–2。
