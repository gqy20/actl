# 04 · 技术选型:为什么是 Rust

日期:2026-09-26

## 1. 结论

**首选 Rust;若团队为 .NET 背景且确定只做 Windows,C# + NativeAOT 为务实备选;Python 仅值得做三天 spike(验证 UIA 行为),Go 出局(COM 互操作无像样生态)。**

## 2. 四个硬约束都指向 Rust

### 2.1 冷启动时间(被 agent 放大的指标)

CLI 是无状态的,agent 循环每一步都是"spawn 新进程 → 执行 → 退出"。一个任务 30 步、每步 3 条命令时:

| 实现 | 冷启动量级 |
|---|---|
| Rust / Go 单二进制 | ~5–20ms |
| C# NativeAOT | ~20–50ms |
| C# JIT | ~100–300ms |
| Python + pywinauto(import 完) | ~500ms–1.5s |

Python 的启动开销在 agent 场景累计为几十秒纯浪费,外加打包分发地狱——直接出局。

### 2.2 UIA/COM 互操作成熟度

- Rust:微软官方维护的 `windows` crate(IUIAutomation 完整 COM binding)+ `uiautomation-rs` 高层封装;
- C#:FlaUI(20 年 UIA 封装生态);
- Go:go-ole 手搓,UIA 无像样 binding——出局。

### 2.3 品类已经用脚投票

agent-desktop、terminator、desktop-cli **三者全是 Rust**。单二进制分发(cargo install / scoop / 直接下载 exe,agent 装机零依赖)+ 大 accessibility 树遍历性能 + COM 生命周期安全,恰是 Rust 甜区。

### 2.4 协议对齐成本低

- agent-desktop(Apache-2.0)的协议契约(JSON envelope、ref 语义、错误码)可直接沿用/移植;
- terminator(MIT,同为 Rust)的 Windows UIA 实现可作代码级参考;
- 选 Rust 等于和整个品类说同一种语言。

## 3. 语言对比一览

| | Rust | C# (NativeAOT) | Python | Go |
|---|---|---|---|---|
| 冷启动 | ~10ms ✓ | ~30ms ✓ | 500ms+ ✗ | ~10ms ✓ |
| UIA binding | 官方 windows-rs + uiautomation-rs | FlaUI(最成熟) | pywinauto | 手搓 ✗ |
| 分发 | 单 exe ✓ | 单 exe(AOT)✓ | 打包地狱 ✗ | 单 exe ✓ |
| 开发速度 | 慢 | **快** | 最快 | 中 |
| 品类对齐 | 三个同类项目同语言 | 无同类 | pywinauto-mcp 系 | 无 |

## 4. 什么情况下选 C#

满足全部三条时,C# + FlaUI + NativeAOT 完全正当:① .NET 背景;② 确定只做 Windows;③ 追求最快出活(开发速度约为 Rust 2–3 倍)。代价:放弃跨平台叙事,与 agent-desktop 协议对齐多一层翻译。

**决策规则:会写 Rust 或愿意学 → Rust;赶时间 + .NET 功底 + 只做 Windows → C#;其余不用纠结。**

## 5. Rust 技术栈清单

| 层 | 选型 |
|---|---|
| UIA | `windows` crate(官方 COM binding)+ `uiautomation-rs`(树遍历/事件) |
| CLI | `clap`(deriver 模式,子命令即契约) |
| 序列化 | `serde` + `serde_json`,envelope schema 带版本字段 |
| 物理输入兜底 | `enigo`(SendInput 级) |
| 进程/系统信息 | `sysinfo` 或 windows crate |

## 6. 架构要点

- **坚持无状态原子命令 + ref scoped to snapshot**:每次 `snapshot` 发新 snapshot_id,ref 仅在本 scope 有效,失效返回 STALE_REF 让 agent 重取——agent-desktop 验证过的核心设计;
- 常驻 daemon(`serve` 模式,desktop-cli 有此形态)是**后期优化项**,性能成为瓶颈时再加;
- MCP 适配层独立于核心：计划在被控 Windows 设备本地启动服务，供其他设备调用本设备能力；传输、认证和故障恢复待设计，见 [ROADMAP](ROADMAP.md)。Linux/macOS 执行后端暂不考虑实现。
