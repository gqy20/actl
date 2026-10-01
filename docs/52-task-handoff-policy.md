# 任务只在开始时交接一次

## 配置

flow YAML 顶层可增加：

```yaml
handoff:
  approval: once_per_run
```

普通脚本使用独立配置（见 `scenarios/verification-task.yml`）：

```yaml
version: 1
handoff:
  approval: once_per_run
```

设置 ACTL_TASK_CONFIG 为配置绝对路径，并为每轮设置唯一 ACTL_TASK_ID。
`scenarios/verify-applications.ps1` 自动生成 ID、设置配置、保存环境和逐调用证据。
不同轮次不复用任务 ID，配置文件不是授权凭证。flow 保存并复用 plan 中的策略。

## 行为

- 首次写操作仍需点击开始，提示明确说明该任务只在开始确认一次。用户明确授权 computer use 代操作时，按 docs/32 核对任务后实际点击；不能伪造授权，代点击不计作人工点击验收。
- 同任务、同策略不因空闲 60 秒或总时长 10 分钟再次请求确认；默认为 expiring，保留原期限。
- 人工输入、主动暂停撤销、停止代次变更、监听故障和显示端重启仍撤销授权。
- 改策略不能静默扩大已有授权；YAML 严格拒绝 authorized 等未知字段及未知策略。
- 前台、具体控件焦点和 --physical 要求不变。不重放结果不明的写操作。
- 显示协议升级为 v8，旧显示端明确版本不匹配，必须正常替换；不清理已有停止记录。

## 验证

- 新测试先因缺少策略 API 失败（target/handoff-policy-before.log），实现后通过。
- 单测覆盖超过一小时仍可复用、不同任务拒绝、人工输入撤销后不能复活、策略升级拒绝、时钟回退和显式撤销。
- YAML 测试覆盖未知策略/字段/版本、重复键、flow 序列化回读及配置 golden；显示协议 golden 同步 v8。
- `cargo xtask gate` 通过（target/handoff-gate.log），release 构建通过。
- 正常关闭并更新已安装显示端，display-status 返回 ready=true、protocol=8、stop_requested=false。
- 真机证据根目录：`target/verification-d3ffd9620f3148e2878f63c78d12bb6b`。
  被测 `target/release/actl.exe` SHA256：`1B9151FBC7D519236D3417296083958B019B082291D51EC0BACBC623F55AACBA`。
- VS Code 首次观察连续 STALE_REF，整轮未通过；关闭即时检查报告 pending 后，单独只读确认 HWND 5769728 和 PID 37032 均已不存在，保留原失败。
- 距上次关闭命令结束 65.4 秒后，沿用同一任务配置继续 Excel；启动成功，但名称框输入因 foreground_mismatch 拒绝。未放宽焦点护栏；文档已关闭、空宿主保留。
- 现有记录未逐次记录用户点击和授权失效的明确原因，不能据此宣称真机整轮仅弹过一次提示；逻辑测试通过与真机交互计数分别报告。
- 同任务继续 Word，正文观察仍报 STALE_REF，窗口正常关闭。Zotero 启动后 UIA list-windows 报 INTERNAL/children，未执行导入写入；另用原生窗口与进程 profile/data 参数核实归属进行清理，保留原失败。

## 待验收

记录并区分每次授权失效原因、实际确认次数；补齐无人工输入时跨软件、长时间只读与等待期间不重复弹窗的真机验收。应用本身的观察和关闭时序问题仍按 docs/51 追踪。

## once_per_run 真机验收（2026-10-01，用户在场协作轮）

临时关闭预授权（验收后已恢复并复验 1.7s 直过），用户真人点击同意卡：

- **一次确认语义**：task-live-1 首写出卡（用户点击开始）→ 同任务后续写入
  立即复用（journal stage=reused），文件内容精确、无重复写入。
- **输入撤销语义（人工在场=接管信号）**：用户持续键鼠活动期间每次写重新出卡
  （shown×3/granted×3 均为用户点击）——external_input 撤销后重卡是设计行为，
  不是 once_per_run 失效；安静后复用恢复。
- **任务隔离**：task-live-2 出自己的新卡；flow（task=live-pause）独立出卡
  一次（shown×1/granted×1），不串用其他任务授权。
- **组内暂停**：流程在等待步边界暂停（reason=pause_requested）→ 恢复 →
  completed，四步 attempts 全 =1（零重复执行），verify-file 全文精确。
- **agent 通道结论（"显示端 agent 可达通道"设计决策的证据）**：合成输入在
  按钮与 Ctrl+Alt+Y 热键两条路均无法授予——注入热键从不触发 user_start
  （四轮自动化实验零命中）；actl 自有带标签注入器因前台护栏与自身需授权而
  递归死锁。授权必须真人，安全模型按设计工作。此前自动化轮观察到的"批准"
  实为用户在场点击。卡面热键提示文案（注册成功与否的判据）待用户口头确认，
  截图已由用户提供。
