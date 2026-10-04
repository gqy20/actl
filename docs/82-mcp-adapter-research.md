# 82 · MCP 适配服务调研与设计（actl-mcp）

> 状态：设计定稿（2026-10-05，用户批准模式 A + 独立 crate + 官方 SDK），实现未开工。
> 授权：PRD §1.3 2026-10-05 行；PRD 2026-09-28 范围调整（被控端显式启动的独立适配服务）。
> 定位：本文件是调研与设计决策记录（照 76/79 先例），实现协议面随 spec 提交落 docs/06。

## 1. 目标与场景

PRD 定向（2026-09-28）：提供**独立 MCP 适配服务，在被控 Windows 设备上显式启动，
供其他设备上的客户端调用本设备能力**；复用本地交接、急停和执行护栏。

场景要点（与典型 MCP 形态相反）：
- 典型 MCP：server 跑在 agent 同机，stdio 本地通信；
- 本场景：**控制端与被控端是两台设备**，actl 的全部护栏（信号条/预告/授权/急停）
  在被控端本地，必须跨边界保持权威。

## 2. MCP 规范现状（2026-07-28 稳定版，本轮核实）

对本设计载重的事实：

1. **无状态化**：`initialize` 握手移除，每请求经 `_meta` 自带协议版本与能力
   （SEP-2575）——每调用自包含，与「子进程派发」模型零妥协。
2. **Streamable HTTP 纯 POST 合规**：GET 流端点已删除（遗留 GET 一律 405 即合规）；
   SSE 变为每请求可选——服务端对每个 POST 直接回单个 JSON 对象是完全合规形态
   （SEP-2575/2596）。这扫清了「同步/简单 HTTP 服务器」的最后障碍。
3. **stdio 传输仍是一等公民**（`server/discover` 兼容探测明确覆盖）。
4. **官方 tasks 扩展**（`io.modelcontextprotocol/tasks`）为轮询式任务句柄——与
   actl 的 flow run id / `flow-status` 语义逐字对应；v1 不实现，标注对齐。
5. **安全要求**：Origin 校验 MUST（防 DNS rebinding）；本地监听 SHOULD 127.0.0.1；
   认证 SHOULD。
6. 传输规范演进快（一年两个大版本）：**协议实现归官方 SDK，自维护成本不可接受**。

## 3. 方案演进（三轮决策，含被否方案与理由）

### 3.1 传输：定稿 模式 A（LAN 直连 + token 配对）；SSH/stdio 为后续可选

| 轮次 | 结论 | 触发修正的事实 |
|---|---|---|
| 初版 | SSH + stdio（同步可手写、认证外包） | 用户质疑**首连不便**——OpenSSH Server 是可选功能（提权安装）、sshd 启动、管理员 authorized_keys 路径坑，约 5 步，劝退级 |
| 二版 | 双模式（A：LAN 直连 HTTP + B：SSH） | 规范核实：纯 POST JSON 同步合规 → HTTP 壳可行 |
| 定稿 | **v1 只交付模式 A**（用户裁定）；模式 B 降为后续可选（rmcp 的 stdio 传输现成，成本近零） | 用户的连接便利优先 |

**模式 A 契约**：被控端一条命令 `actl-mcp --listen 0.0.0.0:7600 [--token <t>]`——
启动时生成并**屏幕打印配对 token**（人工抄到控制端 = 设备主人知情授权的配对仪式，
不是缺陷）；首次监听触发一次 Windows 防火墙系统确认。控制端 `.mcp.json`：

```json
{"mcpServers":{"actl":{"url":"http://<device>:7600/mcp",
  "headers":{"Authorization":"Bearer <token>"}}}}
```

**安全边界如实声明**：明文 HTTP + Bearer token——**仅限可信局域网**；跨公网
明确不支持（token 裸奔），届时上模式 B（SSH）或 TLS 方案。规范要求的 Origin
校验（MUST）照做。

### 3.2 协议实现：定稿官方 rmcp SDK（手写方案被否）

否决手写 JSON-RPC + 同步 HTTP 的两个理由，及否决本身的教训：

- ~~「依赖预算 ≤9」~~——**不是硬阻碍，是拿规则当挡箭牌**：规则有登记通道
  （TECH_STACK §3），CDP-tungstenite 先例（授权登记 + 最小 feature + scope 注明）
  就是路径；
- ~~「async 违 ADR-001」~~——ADR 的本意是保护**桌面命令路径**的同步简单
  （COM/UIA 同步模型、CLI 毫秒级生命周期），不是给边界网络服务上锁。见 §6 修订。
- **真正致命的理由（用户指出）**：为绕依赖而手写协议层 = **制造垃圾**——今天的
  200 行 HTTP 是明天追规范兼容矩阵的 2000 行维护债；MCP 一年两个大版本（本轮
  亲见 GET/会话/initialize 三项被删改），手写版每次演进都是我们的债。**自证先例**：
  CDP 用 tungstenite——传输归库、语义归我们，当时就没手写 WebSocket。

**rmcp 核实结论**（官方 modelcontextprotocol/rust-sdk，3.x，活跃维护）：跟踪
2026-07-28 稳定版并兼容 2025-11-25 及更早；server 端 stdio + Streamable HTTP
（Tower service）双传输齐备；tokio 依赖为既成事实（async 问题走 §6 修订）；
宏驱动工具面（`#[tool]`/schemars schema）。

### 3.3 crate 定界：定稿独立 `actl-mcp`（库 + 自带 bin）

三个候选的裁决维度（编译边界 > 语义诚实 > 登记成本）：

| | 并入 actl-cli 子命令 | 库 crate + cli 薄转发 | **独立 crate 自带 bin（定稿）** |
|---|---|---|---|
| async 隔离 | 最弱：代码路径约定 | 中 | **crate 边界=编译器强制**，rmcp/tokio 想漏进 cli 都漏不进 |
| 主 exe | 背 tokio 体积/编译 | 同左 | **actl.exe 零变化** |
| 语义 | 命令面混入 daemon | 同左 | **桌面操作走 actl.exe、设备接入服务走 actl-mcp.exe**；`ssh host -- actl-mcp` 独立成立 |
| 发布 | 无增量 | 无增量 | release.yml publish 顺序 +1 行（有 v0.2.0 流程先例）；zip 多一 exe |

决定性的是第一行：ADR-001 修订「async 仅限边界适配」时，**边界是 crate 还是
代码路径，决定这条纪律未来五年的可执行性**。zip 多一个文件是登记成本，不是
复杂度成本；MCP 用户（被控端设备主人）装一次无感知，桌面主力用户不替用不到
的网络服务付体积。

## 4. 架构定稿

```
控制端(任意设备)                          被控端(Windows)
┌──────────────────┐   HTTP POST+Bearer   ┌─────────────────────────────┐
│ MCP 客户端        │ ───────────────────▶ │ actl-mcp --listen :7600     │
│ (Claude Code 等)  │  纯 POST/JSON 应答    │  rmcp server(协议/传输/取消)  │
│ .mcp.json: url+  │ ◀─────────────────── │   ├ tools/call → spawn 子进程 │
│  headers         │  tool result          │   │   "actl <cmd>" 全护栏照旧  │
└──────────────────┘                      │   ├ envelope→tool result 映射 │
                                          │   └ ACTL_TASK_ID=mcp-<id> 审计│
                                          │ 信号条/预告/急停:全部本地权威   │
                                          └─────────────────────────────┘
```

双面分工：**协议面归 rmcp**（规范跟踪/双传输/兼容矩阵/取消/进度）；
**资产面归我们**（子进程派发模型 + envelope 语义 + 安全穿越），后者是本设计
真正不可妥协的部分。

## 5. 契约设计

### 5.1 安全穿越（核心红线）

- **适配器是薄翻译层，不做任何权限判断**：每次 `tools/call` spawn 一个真实的
  `actl <命令>` 子进程（与信号条 spawn record-start 同模式）——预告、开始/继续
  本地放行、急停、看门狗、输入锁**全部原样生效**。远端发起的写操作照样要在本地
  信号条上放行：**远程受监督操作是特性**。
- **急停代次本地独占**：远端结构性无法清急停——排除表初版四条：
  `execution-recover`（停止/任务状态修复，急停纪律的结构性延伸）、
  `display-start`（显示端生命周期归本地人）、`desktop-restore`（初版保守排除，
  实现轮可按「动监督机制才出局」原则重议——它有恢复记录护栏兜底）、
  `record-worker`（record-start 派生的内部进程缝，非用户动词）。排除原则：
  **操作本地监督与恢复机制本身的动词出局；用户可见副作用（点窗/打字/剪贴板）
  不出局**——后者是信号条预告的管辖对象。排除不是静默的：远端调用排除动词
  返回明确 PROTOCOL 错误（本地专属动词+原因）。急停后远端调用自动 fail-closed
  （spawn 子进程绑停止代次）——无需为封锁远端额外排除任何命令。
- 配对/认证：模式 A = token 仪式（§3.1）；Origin 校验 MUST。
- 审计：每调用一个 `ACTL_TASK_ID`（`mcp-<随机短id>`），`actl history --task` 全程可查。

### 5.2 能力面（v1 克制；2026-10-05 修订：三层懒加载）

- **一个通用工具** `actl`：`{command, args[]}` → spawn 子进程 → envelope 映射
  tool result（`ok:false` → `isError:true`；error 的 code/hint/evidence 全保留进
  structuredContent——远端 agent 拿到的失败信息与本地完全同构，自愈路径不变）；
- **知识三层懒加载**（上下文成本核算——全量 commands 资源作主入口会被客户端
  自动读取，单会话 15-25K tokens 不可接受；分层后与本地 agent 的 token 经济
  完全同构）：
  1. **工具描述携带紧凑速查**（SKILL.md 命令速查同源，九族一行一条）——随
     tools/list 一次性加载，~700 tokens，仅列远端可用面（排除表已过滤）；
  2. **资源模板 `actl://help/{command}`**——单命令完整帮助（CLI `-h` 四要素
     文本按 URI 暴露，docs/06 §3.5 的既有纪律直接复用）——按需 ~300-500
     tokens/次；排除动词的模板不存在（读不到即面外）；
  3. 全量 `actl commands` JSON **从资源面移除**，仅作为可显式调用的命令为
     机括/调试场景付费；
- 支持面 = 全部命令 − 排除表（§5.1），不做 76 工具镜像、永不漂移；actl 的
  哲学：**技能层携带用法知识，传输层只做搬运**；
- 不实现 MRTR/elicitation：我们的授权模型是本地信号条放行（远端调用发起后被控端
  用户本地点「开始」），不需要客户端侧交互轮次——顺带钉死「远端永远无法自助
  获取权限」。

### 5.3 长操作与并发

- 有界 worker 池（≤4）spawn 子进程：长流程期间 serve 循环不阻塞，`flow-status`
  可并行轮询；
- v1 不做 SSE 流/进度通知（规范可选）：长操作走轮询工具（`actl` 工具调
  `flow-status`），与本地 agent 循环同构。

### 5.4 恢复语义（不新增发明）

连接断开/请求取消 → kill 对应子进程 → run 留 interrupted/uncertain → 重连后
`flow-resume` 走既有恢复链。flow run id 是天然的任务句柄（与官方 tasks 扩展
语义逐字对齐，将来接入是纯增益）。

## 6. 规则修订（随实现 PR 同步落地）

1. **ADR-001 定界修订**：原文「同步代码，不引入 async/tokio」修订为——
   「**桌面命令路径（core/uia/flow/cli/browser）保持同步**；async/tokio 仅允许
   出现在边界适配 crate（actl-mcp），crate 边界由编译器强制」。理由行同步更新
   （原理由对本修订依然成立：COM/UIA 同步模型——修订护的是这个，不是网络服务）。
2. **硬规则 6 依赖登记**：rmcp（官方 Rust SDK）登记 TECH_STACK §3，照
   CDP-tungstenite 先例——版本策略「跟 3.x stable」、feature 最小集（server），
   scope 注明「仅 actl-mcp」；预算口径随之明确：**核心执行路径直依赖 ≤9 不变，
   边界适配 crate 的协议依赖单独登记**（tokio 等传递依赖不腐蚀核心，`cargo tree
   -p actl-core/cli` 不可见）。

## 7. 实施切分

- **轮 1（spec，一次 PR）**：`crates/actl-mcp`（lib+bin）+ rmcp server + 模式 A
  监听（--listen/--token，Origin 校验）+ `actl` 通用工具（描述携带速查表）+
  `actl://help/{command}` 资源模板（按需懒加载）+ 子进程派发 + envelope
  映射 + 危险动词排除表（静态表+契约测试：排除动词返回明确 PROTOCOL）+
  `ACTL_TASK_ID` 映射 + **离线契约测试**（喂 JSON-RPC、断言 spawn 参数与
  映射——CI 可跑）+ ADR/登记/CHANGELOG/docs/06 协议面 + release.yml publish 顺序。
- **轮 2**：取消（断连 kill）、进度通知（可选）、`mcp-setup` 只读诊断（防火墙/
  token/客户端配置行打印）、skills 远程使用指南。
- **轮 3（可选）**：模式 B（SSH + rmcp stdio 传输）、官方 tasks 扩展对齐、TLS。

## 8. 开放问题

1. rmcp 宏工具面是否引入 schemars（纯 derive 无传递污染，可接受；或手写 Tool
   结构避开——实现时定）；
2. `--token` 缺省策略：无参启动生成随机 token 并打印 vs 拒绝启动要求显式
   ——倾向前者（打印即配对仪式），实现轮定；
3. 多客户端并发同一 token 的会话隔离粒度（task id 已隔离，run 锁天然互斥）。

## 9. 规范引用

- MCP 2026-07-28 changelog：无状态化/GET 删除/tasks 扩展/MRTR
  （modelcontextprotocol.io/specification/latest/changelog）
- Streamable HTTP：纯 POST 合规、Origin MUST、取消=断流
  （…/basic/transports/streamable-http）
- 官方 Rust SDK：github.com/modelcontextprotocol/rust-sdk（rmcp 3.x，
  跟踪 2026-07-28 稳定版）
