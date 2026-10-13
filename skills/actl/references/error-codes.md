# actl 错误码恢复手册(envelope 同源;agent 按此自愈)

退出码:`0` 成功 · `1` 业务失败(可恢复)· `2` 协议/参数错误(修命令)。

| code | 含义 | 恢复动作 |
|---|---|---|
| `STALE_REF` | @eN 失效(树变了/窗口重开) | 重新 snapshot;带 `--snapshot <id>` 可校验窗口实例 |
| `NOT_FOUND` | 选择器无匹配 | 用 find 放宽条件;检查 --app 是否唯一匹配窗口 |
| `PERM_DENIED` | 注入被护栏拒绝(物理前台≠目标)或权限不足 | 等桌面空闲;确认目标窗口可被激活;管理员窗口需提权 |
| `TIMEOUT` | wait/锁等待超时 | 重估前置状态;另一 actl 实例持锁时等它结束 |
| `AMBIGUOUS` | 多候选 | 读 `evidence.candidates` 选 ref 直接重试;或 `role:X#N` / `--near`;语义歧义可管道给模型消歧:`actl click name:X --app Y \| actl decide --task '<意图>'`(回显 resolved 块可直接重放,弃权则仍由人/agent 裁决) |
| `NOT_ACTIONABLE` | 元素不可操作(无 pattern/遮挡/offscreen) | scroll/focus 后重试;或 `--physical` 物理点击;对话框输入优先 set-value |
| `ASSERTION_FAILED` | verify/--expect 断言未成立 | 看 `evidence.actual` 与预期的差异,回查前置步骤 |
| `PROTOCOL` | 参数/语法错误(不可恢复) | 修命令行;`actl <cmd> -h` 看用法 |
| `PROVIDER_UNAVAILABLE` | decide 的决策端点不可达/超时/返回不合规(`choice_not_offered`=答了没提供的选项) | 检查 `%LOCALAPPDATA%/actl/decision.json` 的 endpoint 与服务可用性(本地 llama-server/LM Studio 看端口);decide 只影响应答生成,原子命令与流程不受影响 |
| `INTERNAL` | 系统/UIA 层错误 | 重试一次;持续出现附 evidence 报 issue |
| `ABORTED` | 用户紧急停止(Ctrl+Alt+F12/停止文件) | 停止后续计划,询问用户是否继续;清除标志由用户操作 |

## 常见组合场景

- **另存为对话框**:`press ctrl+s --app X --expect window:另存为` →
  `set-value id:1001 "文件名" --app 另存为`(id 失败换 `id:FileNameControlHost`)→
  `press enter --app 另存为` → 磁盘/verify 终验。
- **键入后验证**:不要信窗口标题;`get role:Document --app X --property value` 读回内容。
- **应用挂起(慢)**:命令自带唤醒(snapshot/find 上报 `wake_ms`);若持续慢,先 focus-window。
