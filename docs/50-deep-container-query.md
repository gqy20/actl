# 深树容器定位修复

日期：2026-09-29。原始 ZCode 失败证据保留在 [49](49-local-observation-acceptance.md)，未覆盖或改写。

## 原因与修复

严格 UIA 选择器原来逐节点遍历整个目标窗口以验证唯一性。即使 sidebar 已命中，无关分支超过
40 层也会让查询返回 selector_budget_exceeded，局部观察因此无法开始。

现在对含精确 AutomationId 或 Name 的条件使用原生 FindAllBuildCache 查询（优先 ID），
以 RawView 和原 children/descendants/含根范围取回全部候选，再核对全部组合条件。
候选唯一才继续；不会遇到第一个匹配就返回。查询失败、后续属性失败均保留错误，不回退到模糊路径。
候选超过 5000 仍拒绝；已知角色也通过原生 ControlType 查询；仅名称片段等无精确条件的查询继续受原 DFS 预算限制。

定位与快照采集分开：定位成功不会使全窗口快照变完整，也不会取消局部 depth/limit 截断。
该变化同时用于严格 find、动作定位和局部快照重放；身份核验与桌面交接要求不变。
单个同步 provider 查询可能耗时，不承诺硬超时、恒定工作量或扫描时间改善。

## 复现与验证

- 先增加自有原生窗口的 45 层无关分支与独立目标按钮。旧代码确实失败，日志
  `target/deep-container-before.log`：NOT_ACTIONABLE / selector_budget_exceeded。
- 修复后同一复现通过，`target/deep-container-after.log`。追加原生检查覆盖精确名称、
  within + children、排除容器自身、组合条件不符和深层重复名称歧义。
- 单元测试覆盖完整候选再过滤、重复候选、空结果、provider 失败、后续候选属性失败和候选超限；
  原宽泛遍历错误与歧义契约继续保留。新增候选预算错误 golden，协议同步至 [06](06-design-blueprint.md)。
- release CLI：`E:\explore\computer_cli\target\release\actl.exe`，SHA-256
  `F55B9E229F8E8E4E66B4BC3B45E2AA4EA345E9B7A3B611ED5F999D6E980E7D41`。
  环境头与当前窗口核验：`target/deep-container-environment.json`。
- 最终原生回归 3/3 通过，日志 `target/deep-container-native-final.log`：深树用例、采样脚本端到端、
  release CLI 的范围/截断/绑定重放/失效检查。采样和重放窗口回收且前台保持不变，证据
  `target/local-observation-owned-576/cleanup.json`、`target/local-snapshot-live/run-576/cleanup.json`。
  深树库内测试程序 `target/debug/deps/actl_uia-107b82953eeb6492.exe`，SHA-256
  `9F74F560FFC37C3A99199FEC90C636B7AC5AAF43BD37D5EC3B9B49B33DC89193`。
- `cargo xtask gate` 通过：309 项通过、27 项交互桌面测试跳过；上述 3 项原生用例另行实测通过。
  门禁日志 `target/deep-container-gate.log`。本机 CLI 已替换为同哈希 release 文件，旧文件备份
  `target/deep-container-installed-backup.exe`。execution-status 为 ready/stop=null；当前显示端未运行
  （display-status: companion_missing），未自动授权或启动写任务。

## 真实软件验收边界

本轮原生窗口清单中没有 ZCode，因此没有执行 ZCode sidebar 的新一轮实测。
仍需在 ZCode 打开后使用 `scenarios/local-observation.ps1` 复验。若全窗口依然截断，
报告只可证明局部定位与绑定读回可用，不能声称整体输出削减率或 token 消耗已验收。
历史 provider 状态不能由自有原生窗口测试替代；本批不更新真实应用 pack 的 verified 日期。

## 后续 ZCode 实测（2026-09-29）

用户要求开始验证后，通过 actl launch 的开始交接打开 ZCode 3.14.3.7762，PID 33348、HWND 395506。
启动证据 `target/zcode-deep-launch-01/`；后续仅观察，无业务点击、编辑或输入。
使用已有用户 profile，窗口保留供用户继续使用，不因本次启动推定恢复内容全部归测试所有。
被测 release CLI 路径与 SHA-256 同上。

1. `target/zcode-deep-verified-01/report.json`：sidebar 定位、局部快照和 3 个绑定引用读回均成功；
   首次完整树仍在应用加载阶段（14→605 节点），整体判为 blocked，不计削减验收。
2. `target/zcode-deep-verified-02/report.json`：前后完整/局部内容、容器身份和窗口实例一致，
   无截断，3 个绑定引用读回通过，状态 measured。节点 605→510；元素数组 45,779→39,173 字节；
   完整 envelope 46,023→39,670 字节，减少 13.8%。这是 UTF-8 字节，不是 tokenizer token 数。
   同一轮进程耗时完整 2150ms、局部 2321ms，不支持“更快”的结论。

当前完整树最大深度 36，未重新出现历史 3115 节点的深度触顶页面，因此本轮证明真实 ZCode 的
精确 sidebar 定位与绑定读回可用；超过 40 层的机制修复仍由前述原生复现测试证明。
历史深树页面、虚拟化及 provider 重建继续待验收。只读路线同步到 `skills/actl/apps/zcode.yml`，
未推广为编辑/发送等业务路线已通过。

本轮文档和 ZCode pack 同步后 `cargo xtask gate` 通过，含 11 个 pack / 296 个步骤校验；
日志 `target/zcode-deep-verification-gate.log`。未提交本轮改动。

## 历史超深会话复验完成（2026-09-29）

根据历史树中的会话标题定位到“分析项目Workflow设计与最小闭环规划”，在同一 ZCode 窗口
通过新局部快照绑定的 ListItem 执行 UIA Invoke 打开已有会话；无物理输入、无编辑或发送。
导航证据 `target/zcode-deep-navigation-01/`。用户 profile 和恢复的会话窗口均保留，不做资源清理。

- 首轮 `target/zcode-deep-restored-01/`：完整树 2012 节点并深度截断，局部观察与 3 个绑定读回通过，
  后续局部复查发生 automation_id 通道 STALE_REF / observation_failed。整轮仍保留 blocked，不能改记通过。
- 旧版对照 `target/zcode-deep-old-comparison-01/`：在该超深页面，对同一 sidebar 精确选择器返回
  NOT_ACTIONABLE / selector_budget_exceeded。旧二进制 `target/deep-container-installed-backup.exe`，
  SHA-256 `50F927662768764E4A087D2A267338DD4B05853AF10D26DE289F65C2FE351FF5`。
- 修复版第二轮 `target/zcode-deep-restored-02/`：11 次 CLI 调用全部成功；前后完整树均为 2009 节点，
  均标记 depth_limit；局部 sidebar 前后均为 507 节点且未截断，容器身份、范围和全部局部内容一致，
  3 个 snapshot-bound ref 名称读回通过。窗口/进程实例及停止代次核验通过。

**验收结论：历史真实超深页面中的局部容器定位与绑定读回通过。** 汇总判据和证据索引在
`target/zcode-deep-recheck-summary.json`（status=passed）。完整基线仍被截断，因此原采样脚本正确地
将“字节削减测量”保持 blocked；该结论与局部定位通过分别记账，不计算本轮削减率或 token 数。
真实 provider 瞬态错误没有被吞掉或自动重放写操作，首轮失败保留；此记录不代表虚拟化与所有重建场景已验收。

同步验收文档、ROADMAP 和 ZCode pack 后，`cargo xtask gate` 通过，日志
`target/zcode-deep-recheck-gate.log`。本轮未提交改动。
