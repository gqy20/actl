# actl MCP 远程使用（模式 A）

> 面向两类读者：**被控端设备主人**（启动服务与配对）与**远端 agent**（经 MCP
> 调用本设备）。协议与设计全文见仓库 docs/82-mcp-adapter-research.md 与
> docs/06-design-blueprint.md §7.8（不随包分发，查主仓库）。

## 被控端：启动与配对（设备主人）

```
actl-mcp --listen 0.0.0.0:7600          # LAN 直连;首次触发防火墙确认(预期)
actl-mcp --listen 0.0.0.0:7600 --token 预共享密钥
```

- 默认绑定 `127.0.0.1`（fail-safe：不联网）；**要被其他设备调用必须显式
  `0.0.0.0`**。注：MCP 是**入站**监听；actl CLI 的唯一**出站**网络命令是
  `decide`（判别式决策端点，见 references/error-codes.md 与 SKILL.md），显式
  配置 `%LOCALAPPDATA%/actl/decision.json` 后才可达——远端 agent 侧应知悉；
- `--token` 省略时生成 256-bit 随机 token 并打印配对横幅（stderr）：token、
  客户端 `.mcp.json` 一行、局域网警告——**人工抄到控制端 = 你知情授权这台设备
  被远端调用**（配对仪式本体）；
- 明文 HTTP 仅限可信局域网；跨公网不支持（届时走 SSH 隧道或等 TLS 方案）；
- actl 可执行文件与本服务同目录（zip 并排）或设 `ACTL_EXE`。

## 控制端：接入（agent 侧配置）

```json
{"mcpServers":{"actl":{
  "url":"http://<device>:7600/mcp",
  "headers":{"Authorization":"Bearer <token>"}}}}
```

## 远端 agent 的操作纪律（与本地 agent 同构）

1. **先读工具描述**：`actl` 工具自带九族速查（~700 tokens，随 tools/list 一次
   载入）；
2. **按需读帮助**：用前不熟参数的命令，读资源模板 `actl://help/<command>`
   （四要素：Usage/参数/Example/Errors，~300-500 tokens）——不要读全量
   `commands` schema（机括场景才值得那 15-25K tokens）；
3. **循环纪律与本地全同**：观察→决策→执行→验证；写后必 verify；失败 envelope
   自带 code/hint/evidence，按错误码自愈（STALE_REF→重取快照重试等）；
4. **授权语义**：远端写操作发起后，**被控端的用户要在本地信号条上放行**——
   你的调用会等待；用户拒绝或急停，调用如实失败。这不是超时 bug，是受监督
   远程操作的特性；
5. **本地专属动词**（`execution-recover`/`display-start`/`desktop-restore`/
   `record-worker`）调用会得到明确 PROTOCOL 错误——它们操作被控端本地的
   监督与恢复机制，不开放远端；
6. **审计**：每次调用映射被控端 `ACTL_TASK_ID=mcp-<id>`，`history --task` 可查；
   长流程用 run id + `flow-status` 轮询；断连即取消（kill_on_drop），重连后
   `flow-resume` 走既有恢复语义。

## 故障速查

| 症状 | 判读 |
|---|---|
| 401 | token 不符——抄错或被控端重启换了 token（省略 --token 每次随机） |
| 连接被拒 | 服务没起 / 绑的是 127.0.0.1 / 防火墙没放行 |
| 调用长时间挂起 | 被控端信号条在等用户放行（看设备屏幕），或目标应用无响应 |
| `no JSON envelope` 错误 | 调用参数非法（clap 层拒绝）——读 `actl://help/<command>` 核对参数 |
