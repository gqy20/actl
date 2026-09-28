# 局部观察采样与真实窗口限制

日期：2026-09-29。上一批实现见 [48](48-local-snapshot.md)。本批补采样工具和验收证据，CLI 协议不变。

## 入口与判定

```powershell
pwsh -NoProfile -File scenarios/local-observation.ps1 `
  -Actl target/release/actl.exe -Hwnd <已核对的句柄> `
  -Within 'uia:{"match":{"id":"panel"}}' -Depth 20 -Limit 1000
```

只观察已打开的可见窗口，不启动、还原、聚焦或关闭应用，不注入输入。
报告包含二进制路径/哈希、环境头、进程实例、原始参数、逐调用 stdout/stderr、时间和退出码。
输出目录必须新建，不能覆盖原失败证据。

采样顺序：窗口实例 → 完整快照 → 局部快照 → 最多三个绑定引用读回 → 局部复查 → 完整复查 → 窗口实例/停止代次。
完整树或局部树截断、根节点孤立、范围缺失、内容/容器变化或窗口实例变化均不产生有效削减结论。
观察错误保留原 envelope，报告标为 blocked。measured 只表示可比较，不表示体积一定下降。
元素数组和完整 envelope 的 UTF-8 字节分别统计，允许负削减率；不转换为模型 token 数。
前后相同仍不能证明采样期间从未发生短暂变化，也不能把顺序采样称为原子读取。

## 本轮证据

被测 `E:\explore\computer_cli\target\release\actl.exe`，SHA-256：
`50F927662768764E4A087D2A267338DD4B05853AF10D26DE289F65C2FE351FF5`。
环境 Windows 10.0.26200.0。所有原始树保留在本地 target，不提交应用内容。

| 样本 | 结果 | 原因与证据 |
|---|---|---|
| 自有原生窗口 | measured | `target/local-observation-owned-64060/report.json`；4→1 节点，元素 301→80 字节，完整 envelope 556→576 字节；绑定读回通过，窗口关闭且前台保持不变 |
| ZCode 3.14.3.7762，HWND 8915582 | blocked | 全窗口观察 3115 节点但深度截断；`sidebar` 容器唯一性定位返回 NOT_ACTIONABLE / selector_budget_exceeded；`target/local-observation-zcode-01/` |
| 设置，HWND 6817444 | blocked | 当前仅暴露一个空 Pane，错误假设 role=Window 的定位返回 NOT_FOUND；发现与失败保留在 `target/local-observation-discovery-settings/`、`target/local-observation-settings-01/` |

原生脚本端到端测试通过，日志 `target/local-observation-sampler-live.log`。稳定性、截断、容器重建、
空范围和元数据开销的离线测试进入 xtask gate。真实软件没有通过削减验收，不能据自有窗口替代。
本轮未创建或清理真实应用资源；没有更新应用 pack 的 verified 日期或恢复待验收写路线。

补齐报告中的结构化错误字段后再次执行：ZCode 仍以 NOT_ACTIONABLE 受阻（`target/local-observation-zcode-02/`）；
设置按实际观察到的 Pane 重采样，定位成功，但仅有根节点，因此仍被判为非代表性样本
（`target/local-observation-settings-02/`）。这次局部确认不改写上一轮错误定位记录。
最终 `cargo xtask gate` 通过：307 项通过、26 项交互测试跳过，新增脚本测试通过；
日志 `target/local-observation-gate.log`。本批的原生端到端用例已在门禁之外单独执行通过。

## 下一步

优先解决深树中定位局部容器仍受全窗口遍历预算阻断的问题。需要以本轮 ZCode 失败为依据，
设计可核验唯一性且保留 provider 错误的范围定位方案，不能简单扩大预算或先命中先返回。
之后重跑同一采样入口，再覆盖虚拟化列表与真实控件重建。设置窗口先恢复到可用状态后再测，
本轮不抢焦点尝试修复环境。
