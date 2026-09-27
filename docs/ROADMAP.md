# ROADMAP · 路线图

版本:v0.1 · 日期:2026-09-26 · 时间为相对工期(T = 项目启动日)
关联:[PRD](PRD.md) · [估算](05-implementation-estimate.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

---

## 总览

已完成阶段(Phase 0 spike、Phase 1 M1 → v0.1.0,2026-09-26 发布)的历史明细见
[CHANGELOG](../CHANGELOG.md) 与 [spike-findings](spike-findings.md);本文件只保留未完成工作。

```
Phase 2  T+1 ─ T+3月      M2 认真 MVP     v0.2.0  (45+ 命令,金任务 ≥90%,对外发布)
Phase 3  T+3月起,长期     生产级打磨      v0.3.0  (协议冻结,生态渠道)
```

> 版本口径(2026-09-26 修订):里程碑版本 = 0.<里程碑号>.0——M1 期以 0.1.x 递增
> (已发布 0.1.0–0.1.2),**M2 完成 = 0.2.0,M3 完成 = 0.3.0**;取代本文件早期
> "M2=v0.3.0 / M3=v1.0" 的写法(1.0 保留给协议冻结后的正式稳定版,不再是里程碑承诺)。

> M1 是 M2 的纵向切片,先跑通再铺全——时间线上 Phase 1 与 Phase 2 重叠。

## Phase 2 · M2 认真 MVP v0.2.0(T+1 ~ T+3 月,累计 12–22k 行)

> **批次排序(2026-09-26 定,实证驱动)**:
> **第一批·输入可靠性硬化**(先行,约 1-2 周,不铺新命令)——①输入占用协调
> (doc 09 §6.2:进程间执行锁、长注入分批+批间焦点复核、焦点被抢即停并上报
> 已执行部分;`input_effects` 字段进 envelope)——四次并发真人输入损坏注入的
> 活体实证(docs/spike-findings);②激活感知目标解析(UWP 后台挂起时单命令
> stall ~2s,操作前探测唤醒;cloak/挂起判定依据 [13 §1.1](13-realtime-state-maintenance.md));③`--physical` 规范裁定(06 蓝图)+物理鼠标链
> (doc 09 §6 七步:right-click/drag/hover/scroll 的共同前置);④小项:修饰键
> 只清自己按下的、Toggle/ExpandCollapse/ScrollItem 分发、`snapshot_id` ref
> 版本化校验(后移第二批:依赖快照持久化/会话状态设计,与 fallback 定位链同做)。
> **第二批·命令面与定位链**:45+ 命令补全、fallback 定位链(resolved_by)。
> **追加·显示信号层**(2026-09-26 用户需求,已随第二批落地):状态快照/停止标志/
> 输入锁观测/失联判定四原语 + `actl-signal` 伴随程序(置顶状态条+托盘+
> Ctrl+Alt+F12 紧急停止,状态灯取市场通行色);规范见 06 §7.5。
> **收尾·对外发布**:README 示例、crates.io、scoop;同步发布 Agent Skill
> (SKILL.md + envelope 生成的错误码恢复指南,docs/12 §3.4:品类空缺、成本极低)。
> 协议评审候选(docs/12 §4):AMBIGUOUS/STALE_REF 携带结构化 evidence。

**目标**:达到 PRD §6 全部指标,对外发布(README 示例 + crates.io + GitHub Releases)。

**范围(in)**:
- 命令补全至 45+:right-click/double-click/select/toggle/scroll/drag/hover、key-down、resize-window、notifications、`verify` 族、`extract`、`wait` 完整谓词语法
- `wait`/`verify` 实现约定([13 §4](13-realtime-state-maintenance.md)):轮询制,默认值取生态共识——间隔 ~100ms、窗口/元素级超时 3–5s、text 类指数退避 200ms→1s、动作后 settle ~500ms;窗口状态判定走物理层快照组合(GetGUIThreadInfo/DWMWA_CLOAKED 等,§4.3),不引入事件订阅
- **fallback 定位链**(primary UIA 路径 → 名称模糊+锚点 → role+序数,`resolved_by` 上报)——RPA 借鉴核心([03 §3](03-rpa-vs-quicker.md))
- skeleton 投影(token 削减 ≥70%);evidence(失败附局部树/截图引用);`click --expect`
- 集成测试 fixtures(计算器/记事本/ms-settings);envelope 契约测试
- 分发:cargo install、GitHub Releases 单 exe、scoop manifest

**收尾状态(2026-09-26,v0.1.2 并入)**:README 示例化 ✓ · scoop manifest ✓ ·
Agent Skill ✓ · crates.io 已发布且 tag 自动同步(三 crate;cargo install actl-cli)✓ ·
**US1 十次重跑 10/10(100%)** ✓ · NFR:P95 73.3ms<80 ✓(P50 54.9 微超 50,信号层
状态写的有意权衡)· 信号条交互(菜单/品牌图标/拖动+位置记忆/字重打磨)✓ ·
时序评估闭环(Timing 旋钮 + 画像/扫描 fixtures + 三默认值维持,docs/timing-2026-09.md)✓ ·
外部机器复现 US3 待非开发机验证。

**收官批进展(2026-09-27,v0.1.4 已发;Office/微信场景见 docs/14)**:命令面 27/31
(wait 谓词/select/toggle/paste;list-notifications 搁置 M3,list-surfaces 待裁定)· 窗口
治理(close/focus --pid/--hwnd、batch windows_created+blocked_by 弹窗闸门、fixtures trap
清理、GUI 禁 taskkill)· launch App Paths 解析 · help 全命令四要素 · 信号条重设计
(波纹/自适应/实时耗时/Per-Monitor DPI)· regress 一键回归 · Office 三闭环双应用
(Excel GridPattern 直读+剪贴板直灌;Word TextPattern;docs/14)· 微信核心通道全通
(发送/搜索/文件/联系人/朋友圈/@人/群信息;pack 路线+quirks)· 应用档案 pack v1
(excel/word/wechat + lint-packs + actl×collector 分工协议)。
0.2.0 剩余:正常日 timing/P50 复测、金任务 10 轮全量回归、外部机器 US3(用户侧)、actl-signal.rs 拆分(959 行已过 800 警告线;壳/轮询/渲染/几何四模块,登记于 docs/11 §14)。

**追加批·浏览器内容通道(2026-09-27,spec)**:`snapshot --view content`(内容域
投影,chrome 整枝丢弃,与 skeleton 组合)与 `wait --stable`(settle 谓词)——
真实会话浏览器赛道的机制地基(Chrome 136 起默认 profile 禁 CDP,UIA 是唯一
非视觉通道);browser pack + 场景实测(docs/16)随后跟进,PRD §4 浏览器形态
token 豁免项待实测数字复核。

**退出标准**:
- 金任务集全过,US1"跨 3 应用数据搬运"成功率 ≥ 90%(10 次重跑);
- 冷启动、输出体积、token 消耗三项 NFR 达标(PRD §4);
- 外部机器(非开发机)按 README 复现 US3 成功。

## Phase 3 · 生产级 v0.3.0(T+3 月起,长期)

**目标**:协议冻结,靠 Issue 驱动打磨长尾。

候选方向(按社区反馈排序,不预先承诺):
- 长尾 workaround 库(应用特化 pack,参考 desktop-cli 的 application packs 思路)
- `serve` 常驻模式(协议不变;收益主项是增量观察 `changed since <snapshot_id>` 与定向应答,次级才是冷启动摊销与 wait 提速——设计原则见 [13 §4.5](13-realtime-state-maintenance.md):本地状态富度与协议面解耦、禁 eager 预取、serve 挂掉自动回落单发);可选事件加速:UIA/WinEvent 订阅仅作 wait 触发器(uiautomation crate `event` feature,零新增依赖),前置条件与防御清单见 [13 §4.4](13-realtime-state-maintenance.md)——不维护常驻树、不空闲全局订阅;推迟理由 = 防御工程 + 故障模型 + 会话语义定案(ref 稳定性/快照持久校验),与 token 无关
- 录制器 → agent 可复现"操作轨迹"(OpenAdapt 思路)
- Linux(AT-SPI2)/ macOS 后端(协议层已平台无关)
- MCP 薄壳(独立 crate,不进核心)
- 显示层面板化(docs/11 Phase B/C:介入卡/任务面板/Narrator)——先以 HTML/TS 原型做设计验证(不进发布链);面板化前按 [11 §14](11-desktop-interaction-and-display-research.md) 决策门 spike“现有 Rust 壳 + WebView2 内容层”(拉取协议不变,壳继续拥有窗口/焦点/热键/托盘);不采 Tauri(tokio,ADR-001 冲突)、不采 Electron(体量)

**v0.3.0 发布标准**:envelope 协议评审冻结;错误码全集文档化;破坏性变更仅随版本号递增
并登记 spec(0.x 期破坏性演进常态,冻结后不再静默破坏);有一篇可复现的端到端教程。

---

## 里程碑速查

| 里程碑 | 版本 | 核心交付 | 验收 |
|---|---|---|---|
| M2 | v0.2.0 | 45+ 命令 + fallback 链 + 分发 | 金任务 ≥90% + 三项 NFR |
| M3 | v0.3.0 | 协议冻结 + 长尾打磨 | 发布标准如上 |

## 排期原则

1. **金任务先行**:每个 Phase 的验收任务在开发前定义,不为凑功能延后验收;
2. **范围铁律**:PRD 非目标清单(§1.3)膨胀 = 项目失败的第一原因;
3. **UIA 黑洞预算**:按"写 1 行调 3 行"留时间([05 §5](05-implementation-estimate.md)),进度落后先砍命令面广度,不砍可靠性特性(fallback/verify/evidence)。
