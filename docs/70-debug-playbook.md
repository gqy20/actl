# 70 · 调试手册（debug playbook）

来源：2026-09-29/30 四软件整批六轮实战与单链攻坚（全过程见 [68](68-combined-batch-first-round.md)）。
本文只沉淀**可复用的方法与家族结论**，应用特定知识在各 pack 的 quirks。

## 1. 调试阶梯（从最小复现到整批）

1. **单步手工复刻**：用 actl 命令直接跑（launch/type/press/verify），现场逐步观察。
   今晚 Zotero 就是靠这个破的：每个疑点步骤（导入、搜索提交、导出对话框）先手工验证通过，再进 workflow。
2. **单应用 flow 全链**（新任务 ID）：模板级验证，覆盖引擎语义（recovery/before/verify）。
3. **组合整批**（新任务 ID）：验收口径，桌面空闲要求最高（见 §5）。

**冻结计划铁律**：flow 的计划在创建时冻结，YAML 修正后旧任务**永远跑旧路线**——
重试前先确认修复是否已在新任务里验证过。今晚 verify4/verify8 的 export 重试
各废一轮才想起这条。`--retry-step` 只复活"最新 uncertain 步"（一次续跑一个），
更早的 uncertain 需各自显式指定。

## 2. 观察瞬态家族与耐心语义

「动作已生效、观察未跟上」是贯穿六轮的失败主族（docs/65 同源）。各核验命令的耐心现状：

| 命令 | 耐心 | 说明 |
|---|---|---|
| verify-file / verify-rows / verify-closed | settle 5s | 家族基准（verify-rows 缺失报 NOT_Found 可沉降，歧义立即失败） |
| verify-window | settle 15s | 拆窗实测 3-15s（Office 负载相关，孤立 2.8-4.2s） |
| wait --gone | 60s 预算，观察期错误视为"仍在" | 垂死窗口的 UIA 查询会抖 STALE_REF/INTERNAL |
| wait --element/--stable | 轮询至超时 | 天生耐心 |
| verify --value（element_text） | settle 5s | 重渲染期读到旧值表现为内容不符 |
| verify-clipboard | settle 5s（空/旧内容均重读） | ctrl+c 送达与剪贴板可读之间的竞态 |

引擎侧：观察恢复（recovery 预算）的 retryable 集合 = StaleRef/Timeout/NotFound/**Internal**
（Internal 纳入是 2026-09-30 实证：冷启动窗口 UIA 抖动，有界 3 次/15s 不吞真错误）。
**给易瞬态的步骤配 recovery 预算是模板标配**（vscode 7 步、zotero 9 步、excel 13 步已配）。

**`wait --stable` 手写参数建议**（third-hand 消费级 settle 轮询的对照参数，见
[08 增补](08-agent-cli-jev-design-proposal.md)；`--stable` 采样间隔 settle 250ms、
连续两次不变即判稳是引擎默认）：
- 走完一次写/导航后要判稳，`--timeout` 给 **≥3000ms**：动画/搜索结果类常见 1-2s
  中间重绘，250ms×2 的默认口径会以"稳定"接受一个中间态——预算小于 3s 的
  settle 等于没等；
- 判稳通过≠业务生效：`--stable` 采样通道是 text > value > name 的元素视图，
  配 `verify`/`verify-file` 收业务断言（ensure-element-value.yml 的两段式）；
- 双快照对照（`snapshot-diff`）前先 `--stable`，否则差分里全是过渡帧噪声。

## 3. Windows 原生表面 vs 引擎正斜杠约定

引擎把 `${run.dir}` 统一正斜杠（记录一致性），但三个原生表面拒收正斜杠，全部实证过：

| 表面 | 症状 | 解法 |
|---|---|---|
| Zotero `-datadir` | 启动只弹错误框，无主窗口 | `${run.dir_native}` |
| Windows 保存对话框文件名框 | 「文件名无效」/文件不落盘 | `${run.dir_native}` |
| Firefox 系命令行解析 | 未知 | 同上预防 |

**通用规则**：传给原生应用的路径参数（launch args、对话框输入）一律
`${run.dir_native}`+反斜杠后缀；仅 Rust 侧 IO（verify-file --path）两种皆可。
新增 workflow 时审查所有出流程的路径参数。

## 4. 转义与编码纪律（bash 工具链）

- **bash 内联命令禁含**：中文（GBK 污染，AGENTS 已有）、**反斜杠路径、`$`变量**——
  后两者是 2026-09-30 新增实证：`"C:\\...\\$T.xlsx"` 经工具层+双引号组合产出
  `...$T.xlsx`，Excel 收到残渣 `X:/.xlsx` 弹「无法找到」。
- **一律写脚本文件**（.cjs/.ps1/.py）执行；脚本内路径用正斜杠或 join。
- 症状速查：launch 返回 ok 但应用弹「无法找到 X:/…」→ 先查参数被转义层吃掉，
  再怀疑应用本身。

## 5. 共享桌面与无人值守

- **跑批前置检查**：`actl display-status`（ready/covered_or_offscreen）+ 前景窗口标题
  （锁屏=「Windows 默认锁屏界面」，所有动作正确 fail-closed）+ `wait --idle`（存在但
  尚无 workflow 使用）。诊断三段法第一条永远是环境（锁屏/遮挡/前台竞争）。
- **前台竞争**（用户活跃）会让开局步骤死于 foreground_not_acquired；被打断后恢复
  链路（暂停→出卡→续跑→复活同段 skipped）已多次实证可用。
- **外来程序注入**（hook_foreign_injected 计数）会触发让行且不赦免（键盘类）——
  用户的另一个 agent 会话/带热键工具是常见源，journal consent_decision 可证。

## 6. 证据采集实践
- **被测二进制的证据化**:`cargo xtask evidence` 出 RELEASE 二进制并落 FNV-128 指纹(`target/evidence/binary-log.txt`)——证据轮入口与诊断循环分离,见 §8。

- **用户截图两次成为决定性证据**（Zotero -datadir 错误框原文、X:/.xlsx）——用户在
  现场时请其截图是最低成本的根因定位手段；失败现场稍纵即逝，事后枚举常已变样。
- journal 三件套：`history --timeline`（时间线）、consent_decision（输入台账含
  hook 计数）、workflow_sequence（序列内逐步 ok/error）。
- 诊断脚本现场清单（值得沉淀为工具，见 ROADMAP）：窗口标题监视器（500ms 轮询
  谁匹配某串）、close→gone 计时器、隔离应用实验台（flow-resources 路径 launch+就绪）。

## 7. 症状 → 根因速查（本轮实证）

| 症状 | 实证根因 |
|---|---|
| verify-window absent 反复失败、事后窗口却没了 | 单发判定撞 3-15s 拆窗（§2） |
| 步骤动作 delivered 但 verify NOT_ACTIONABLE 且盘上无变化 | 动作实际生效、核验通道被挡（docx 锁/对话框未开/文件名被拒） |
| launch ok 但应用弹「无法找到 X:/…」 | bash 转义层吃掉路径（§4） |
| --retry-step 后仍旧行为 | 冻结计划（§1） |
| 全部步骤同时 NOT_ACTIONABLE | 锁屏/显示端被遮挡（§5） |
| Excel 关不干净、壳越积越多、系统鼠标转圈 | 加载项钉住空壳；WM_CLOSE/COM Quit 均无效；重启清 |
| 选择器间歇 AMBIGUOUS | 动态插入的同名元素（通知条/集合树行/弹出的 List 本身），role/automation_id 精确化 |


## 8. 双循环二进制纪律（2026-10-03 操作化）

来源:探索模式跨窗链攻坚(发现 #3–#6,docs/77)的流程复盘——**慢循环的根因不在工具链,在循环结构**。

### 8.1 三个成本层与量级(实测)

| 层 | 实测 | 根因 |
|---|---|---|
| release 重编译 | 123–173s/次,攻坚轮 ~8 次 ≈ 20 分钟纯编译 | release 默认关增量 + LTO 全量重做代码生成与链接 |
| actl 调用链 | list-windows 558ms(抖动 10s)、snapshot ~450ms、launch 后固定 sleep 3-4s | 每命令独立进程(spawn+COM 初始化+全窗口枚举);固定 sleep 拍脑袋 |
| 循环结构 | 每轮只验一个假设 → N 假设 N 次重编译 | 诊断探针一轮只埋一个 |

关键认知:**诊断轮不需要 release**——actl 运行时大头是 UIA COM(百毫秒级),二进制优化等级对诊断速度影响可忽略;release 的唯一价值是证据轮的产物质量与哈希记录。

### 8.2 双循环模型(工具化入口)

- **诊断循环**:`cargo xtask dev` → 增量 DEBUG 构建(实测无改动 0.8s/小改动 ~4s)→ 用 `target/debug/actl.exe` 跑诊断场景 → 复盘。试错、语义验证、探针迭代全在这里。
- **证据轮**:`cargo xtask evidence` → RELEASE 构建(全量 ~100-170s)+ 二进制 FNV-1a 双通道 128 位指纹追加落 `target/evidence/binary-log.txt`。每轮验收**一次**;调用本身即"进入证据轮"的显式声明。

禁止在诊断循环里 `--release`(40× 差距)。计时数字随命令打 stderr(与 gate 同纪律,同环境头内比较)。

### 8.3 循环内规则(与 AGENTS 验收纪律 1/2/6/7 对应)

1. **一轮多假设**:重编译前列全待验假设,一轮埋多探针(多处 eprintln/ACTL_TRACE=1)。攻坚实证:6 个真机发现理想 2-3 轮收齐,实跑 8+ 轮。
2. **batch 探测**:3+ 顺序调用走 `batch`(单进程共享 COM/snapshot)。
3. **事件等待**:`wait --window` 替代 `sleep N`。
4. **真机专属语义尽早进 fixture**:MockHost 边界在 Host trait,CLI 层语义(ref 回放、编号口径、定位通道行为)只有真机暴露——发现 #4/#5/#6 全是这个类。此类问题补 fixtures 轨契约测试(如 snapshot→`click @eN --snapshot` 回放断言),把"慢循环里发现的类"变成"测试里拦住的类"。

### 8.4 教训(诚实记录)

2026-10-03 攻坚轮,AGENTS 里白纸黑字的 debug-first 纪律被违反 8 次——原因是把"证据二进制"和"诊断二进制"混为一谈(每轮修完都重建 release 想拿"可记录的产物")。规范化答案不是更严格的文字,而是**把两个循环变成两个工具入口**(dev/evidence),让正确路径成为最容易走的路径。
