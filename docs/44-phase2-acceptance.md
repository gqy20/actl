# Phase 2 无人值守实施与验收

日期：2026-09-28（持续运行至次日的记录以各报告时间为准）。
范围：ROADMAP Phase 2 的可靠性修复、可无人值守验证及待人工验收清单。
**当前不是 Phase 2 全部通过**：人工交接、真实业务写入、非开发机和性能门槛仍有待办。

## 已实现

1. 输入监听：保留低级 hook，增加独立 Raw Input 键鼠活动计数；核验注册归属与消息泵心跳。
   失效撤销授权，健康恢复不自动续跑。不存储按键、文本或坐标，也不发送探针输入。
2. UIA 窗口清单、RuntimeId 基线、新窗口等待和 PID 定位迁移可失败枚举，观察错误不吞并为缺席。
   修复空闲查询失败被当成无人操作；空闲等待响应 flow 暂停/停止。
3. 历史清理按跨进程锁和成功时间限频，避免每个命令扫描数千日志；保留失败可重试和活跃文件保护。
4. `cargo xtask regress --unattended` 只执行显式登记用例。其余保留 skipped，`complete=false`，不伪造全量通过。
5. `scenarios/phase2-readonly.ps1` 保存环境、二进制哈希、逐调用输出、新进程计时、原生窗口身份及 full/skeleton/full 对照。
   进程时间截止退出，不包含证据序列化；投影报告只计算 UTF-8 字节。单节点和空 skeleton 标为非代表性样本。
6. 浏览器原型报告保留有界底层异常原因链；不改变写入 uncertain 或禁止自动重放的约束。

## 本地证据

产物根目录：`target/phase2-20260928/`。原始 UI 树仅存本地，不提交私人窗口内容。
Windows 10.0.26200，22 个逻辑处理器。PowerShell 进程 PATH 前置 `C:\Users\gqy17\scoop\shims`，
等价于用户指定的 Bash `export PATH="/c/Users/gqy17/scoop/shims:$PATH"`；本轮使用 PowerShell，不内联 Bash 中文。
通过独立 PowerShell 线程成功申请 SYSTEM_REQUIRED / DISPLAY_REQUIRED，运行期保持唤醒；收尾释放请求并发送关闭屏幕指令。

| 验证 | 结果与证据 |
|---|---|
| 空闲读取失败复现 | `idle-before.log`：1 项失败；修复后 UIA 测试通过，加入错误 envelope golden |
| 窗口观察复现 | `windows-before.log`：提取旧吞错语义后 4 项失败；修复后通过，加入窗口错误 golden |
| 清理重复扫描复现 | `retention-before.log` 失败；修复后重复、到期、时钟回退、并发和失败重试测试通过 |
| Raw Input 注册生命周期 | `raw-registration-live.log` 1 项通过；仅注册、核验、释放，不是人工接管测试 |
| 无人值守流程回归 | 首轮 `regress-unattended.log` 和最终 `regress-final.log` 均为 1 项 passed、18 项 skipped；最终报告 `target/regress-b55f5337e9924b7586e7ee47a06d8c90/report.json`，complete=false；非整套通过 |
| 浏览器离线 | `browser-offline-final.log`：12 项通过 |
| 浏览器第一次真机 | `browser-live/report.json`：绑定通过、iframe 写后核验失败，保留 uncertain；`browser-recheck.json` 只读查到 untouched，没有重放 |
| 浏览器第二次真机 | `browser-live-2/report.json`：6 项检查通过，已正常关闭自建浏览器；第一轮遗留资源单独核对并正常关闭，见 `browser-cleanup-confirmed.json` |
| 原生/UIA 清单、显示诊断、空闲查询 | `readonly-4/report.json`：通过；没有启动或关闭用户应用、没有键鼠注入 |
| 双客户端 UIA 对照 | `uia-compare-settings/report.json`：采集成功，精确 HWND/PID/进程实例前后相同；actl Raw 与 winapp Control 均只见一个 Pane，不据此认定漏控件或完整业务可操作 |
| 冷启动与投影 | 见下节，未满足全量 NFR 验收 |
| 最终门禁 | `gate-delivery.log`：283 项 Rust 测试通过、5 项忽略；fmt/clippy/pack/脚本护栏与打包测试通过；前一轮 gate-final.log 同样通过 |
| 真实发布包演练 | `package-final.log`：使用真实配套二进制构建 `package-final/actl_0.1.6-phase2-validation_windows_amd64.zip`，归档内容校验通过；解包后版本查询及附带业务 YAML 的 flow-validate 通过，见 package-flow-preflight.json；未发布 |

最终被测二进制位于 `target/validation/release/`：

- actl.exe：`20733A94D55C003C2579B1F9F997CCF8B9BE1BBBCE81C880DFDC5DEE13EE40D6`
- actl-signal.exe：`AC8C106CAAC63ADCBDE8D5ED0ED5AAD0B55A23668FB06354E5134C201DB08CEE`

未覆盖 PATH 中已安装版本；不将本地验证产物视为发布版或已完成安装验收。

### 性能结果与计时口径

- `readonly` 首轮的 P50 699ms / P95 842ms 包含证据保存；该轮还存在 `--idle 0` 参数错误、可选 minimized 字段处理错误，保留失败记录。
- `readonly-2` 和 `readonly-3` 为分阶段实现后的诊断数据，仍是旧计时口径，不与最终进程时间直接比较。
- `readonly-4` 用修正后的进程退出计时，100 次 status 的 P50 **97ms** / P95 **131ms**，未达到 PRD 的 50ms / 80ms。
  被测 SHA256：`6B1EB6553E7622BE9BB68531EC8FE353340BA4F369F678C3DA08DDDC4FD4386D`。
- 两个已存在 UWP 窗口的 snapshot 为 1755ms / 168ms；慢窗口报告 wake_ms 约 1600ms。
  两者均只有单节点，skeleton 为空，属于非代表性样本，不能拿约 96% 的字节削减率宣称深树 token 门槛通过。
- 性能数据只在本环境头内解释。当前不能以缩减日志、关闭护栏或更改阈值取得通过；正常负载、可操作深树和最终二进制仍需复验。
- 最终二进制 `performance-final/report.json`：同一脚本先交替测旧/新各 20 次，再补新版本至 100 次。
  旧二进制 20 次 P50 818ms / P95 1033ms；最终二进制 100 次 P50 **133ms** / P95 **196ms**，仍未通过 NFR。
  开始阶段与本机打包工作有短暂重叠，不宣称这是干净机器基准；保留全部样本和两个哈希，不能与前几轮不同口径数据混算。

## 留待人工或专用环境验收

下表逐项保留为未完成；模拟对象测试、注册成功和只读采样不代替这些结果。

| 范围 | 待执行步骤与验收条件 | 入口 / 前置条件 |
|---|---|---|
| P0 急停生命周期 | 正常关闭/重启无新急停；显式急停跨重启保留；详情解除撤销主授权；旧任务不自启 | docs/35；真人操作显示端 |
| P0 交接恢复 | 人工开始→主动暂停/键鼠接管→右侧继续→业务读回；覆盖未执行、uncertain 和多任务 | docs/32、39；人在场，独立任务标记 |
| P0 定位 | 同名控件、唯一容器、直接子级/后代、控件重建、flow 恢复重新定位 | 组合定位 fixture；不得自动授权 |
| P0 监听冗余 | 在隔离诊断环境使低级 hook 失效，真实键鼠仍让行；撤掉 Raw Input 注册或停消息泵时拒绝写入；恢复后重新授权 | 不通过注入假输入充当真人验收；RDP/锁屏另记 |
| P0 显示状态 | 后台/占用/等待/暂停/让行/故障文字和按钮一致；隐藏、屏外、多屏 DPI、显示端失联和重启 | 真实显示器与交互桌面；不将单屏缩放单测记为多屏通过 |
| P1 观察故障 | 空值、深度/数量截断、控件重建、虚拟化、Grid 读取失败、provider 故障 | docs/37；系统应用独立 fixture 与注入式故障测试分别记账 |
| P1 外部对照 | 在可操作页面比对树视图、深度、属性和焦点；补 Blender/Photoshop/Minecraft 可用环境 | docs/36；本次单 Pane 样本不替代完整场景 |
| P1 业务 | Edge/Chrome/夸克/Zotero/腾讯会议/VS Code/网易云/Office 的遗漏路径与输入后读回 | `cargo xtask regress --list` 后选具体 case；既有账号/接收人信息不得猜测 |
| P1 路线比较 | 数字、搜索、表格、日期时间控件的键盘/点击耗时、成功率、焦点与恢复成本 | 同环境、同任务、明确 `--physical`，不预设键盘更快 |
| P2 金任务 | US1 固定环境 10 次，至少 9 次成功；人工交接和受阻另记 | 当前没有完成本轮 10 次全量任务 |
| P2 NFR | 正常负载重新测冷启动、可操作窗口 snapshot、默认输出与深树 skeleton 削减 | 修正计时脚本；不改阈值，不把空树作为合格样本 |
| P2 外部机器 | 非开发机按 README 完成 US3，核对 CLI/显示端配套版本及技能加载 | 需要另一台 Windows 机器；本机解包检查不能代替 |
| P2 发布 | 全部退出标准通过后整理版本、产物、CI 与 tag | 本次不升版本、不发布；ROADMAP 未验收项保持待办 |

## 复测入口

```powershell
$env:PATH = 'C:\Users\gqy17\scoop\shims;' + $env:PATH
cargo xtask gate
cargo xtask regress --unattended
cargo xtask regress --list
# 人在场并核对前置条件后，选择具体 case，不盲目执行全套写入。
# 只读采样不带 Hwnd 时仍可测 status；界面采样须填入本次观察的句柄。
pwsh -NoProfile -File scenarios/phase2-readonly.ps1
uv run --locked --project prototypes/browser python -m unittest discover -s prototypes/browser -v
```

急停、暂停、业务窗口和私有数据均不为通过验收而清除；资源归属不明时保留现场。
