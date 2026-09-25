# ROADMAP · 路线图

版本:v0.1 · 日期:2026-09-26 · 时间为相对工期(T = 项目启动日)
关联:[PRD](PRD.md) · [估算](05-implementation-estimate.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

---

## 总览

```
Phase 0  T+0 ─ T+1周      spike + 脚手架          (验证可行性,产出坑清单)
Phase 1  T+1 ─ T+5周      M1 激进 MVP     v0.1.0  (20 命令,两个金任务通过)
Phase 2  T+1 ─ T+3月      M2 认真 MVP     v0.3.0  (45+ 命令,金任务 ≥90%,对外发布)
Phase 3  T+3月起,长期     生产级打磨      v1.0    (协议冻结,生态渠道)
```

> Phase 1 与 Phase 2 时间重叠:M1 是 M2 的纵向切片,先跑通再铺全。

---

## Phase 0 · Spike 与脚手架(T+0 ~ T+1 周)

**目标**:用最小代价验证五个最险场景,把"未知未知"变成坑清单;搭好工程骨架。

**范围**:
- Rust probe 验证(`crates/actl-uia/examples/probe.rs`,实测结论见 [spike-findings.md](spike-findings.md)):① Electron 应用 UIA 树质量;② 管理员窗口/UIPI 跨完整性级别行为;③ 中文 UIA 树;④ Canvas/游戏(UIA 不可达,物理兜底是唯一路径);⑤ RDP 会话内行为
- Rust workspace 脚手架(已完成:五 crates、CI、hooks、envelope v1 契约测试)
- 金任务集定义文档化([07-golden-tasks.md](07-golden-tasks.md))

**退出标准**:坑清单入库(每项:场景、现象、绕行方案);CI 绿;uiautomation crate 采用/弃用决策。

**风险出口**:若 ②(管理员窗口)证明无法安全绕行 → 降级为文档化的已知限制,不阻塞后续。

## Phase 1 · M1 激进 MVP v0.1.0(T+1 ~ T+5 周,5–8k 行)

**目标**:纵向跑通"snapshot → 定位 → 操作 → 反馈"全链路,自己每天用。

**范围(in)**:
- 命令(~17):`snapshot` `find` `get` / `click` `type` `press` `set-value` / `launch` `focus-window` `close-window` `list-windows` / `get-clipboard` `set-clipboard` `status` `commands` / `wait --element` / `batch`
- 协议:envelope v1、错误码初版、`@eN` ref + snapshot_id、STALE_REF(无 fallback 链)、退出码三态
- UIA 后端:树遍历 + 可交互判定 + Invoke/Value/Selection pattern 分发;物理兜底 `--physical`(仅 click/type/press)

**范围(out,铁律)**:fallback 定位链、verify/extract、skeleton、evidence、通知命令。

**退出标准(金任务)****:
1. "打开计算器,完成 `123×456+789` 并读出结果" 10/10;
2. "记事本输入多行文本,ctrl+s 保存到指定路径,verify 文件存在" 10/10;
3. 冷启动 P95 < 50ms(100 次采样)。

## Phase 2 · M2 认真 MVP v0.3.0(T+1 ~ T+3 月,累计 12–22k 行)

**目标**:达到 PRD §6 全部指标,对外发布(README 示例 + crates.io + GitHub Releases)。

**范围(in)**:
- 命令补全至 45+:right-click/double-click/select/toggle/scroll/drag/hover、key-down、resize-window、notifications、`verify` 族、`extract`、`wait` 完整谓词语法
- **fallback 定位链**(primary UIA 路径 → 名称模糊+锚点 → role+序数,`resolved_by` 上报)——RPA 借鉴核心([03 §3](03-rpa-vs-quicker.md))
- skeleton 投影(token 削减 ≥70%);evidence(失败附局部树/截图引用);`click --expect`
- 集成测试 fixtures(计算器/记事本/ms-settings);envelope 契约测试
- 分发:cargo install、GitHub Releases 单 exe、scoop manifest

**退出标准**:
- 金任务集全过,US1"跨 3 应用数据搬运"成功率 ≥ 90%(10 次重跑);
- 冷启动、输出体积、token 消耗三项 NFR 达标(PRD §4);
- 外部机器(非开发机)按 README 复现 US3 成功。

## Phase 3 · 生产级 v1.0(T+3 月起,长期)

**目标**:协议冻结,靠 Issue 驱动打磨长尾。

候选方向(按社区反馈排序,不预先承诺):
- 长尾 workaround 库(应用特化 pack,参考 desktop-cli 的 application packs 思路)
- `serve` 常驻模式(性能优化,协议不变)
- 录制器 → agent 可复现"操作轨迹"(OpenAdapt 思路)
- Linux(AT-SPI2)/ macOS 后端(协议层已平台无关)
- MCP 薄壳(独立 crate,不进核心)

**v1.0 发布标准**:envelope 协议评审冻结;错误码全集文档化;破坏性变更仅走 semver major;有一篇可复现的端到端教程。

---

## 里程碑速查

| 里程碑 | 版本 | 核心交付 | 验收 |
|---|---|---|---|
| M0 | — | 坑清单 + 骨架 + CI | 五场景验证完毕 |
| M1 | v0.1.0 | 20 命令全链路 | 两个金任务 10/10 |
| M2 | v0.3.0 | 45+ 命令 + fallback 链 + 分发 | 金任务 ≥90% + 三项 NFR |
| M3 | v1.0 | 协议冻结 + 长尾打磨 | 发布标准如上 |

## 排期原则

1. **金任务先行**:每个 Phase 的验收任务在开发前定义,不为凑功能延后验收;
2. **范围铁律**:PRD 非目标清单(§1.3)膨胀 = 项目失败的第一原因;
3. **UIA 黑洞预算**:按"写 1 行调 3 行"留时间([05 §5](05-implementation-estimate.md)),进度落后先砍命令面广度,不砍可靠性特性(fallback/verify/evidence)。
