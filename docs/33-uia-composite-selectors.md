# UIA 组合定位：实现与验收

日期：2026-09-28。协议见 [06 §3.2](06-design-blueprint.md)，流程示例见
[calculator-composite.yml](../fixtures/flows/calculator-composite.yml)。

## 已实现

- `uia:<JSON>`：match 条件 AND，role、id、精确 name 或 name_contains。
- within 唯一容器，children/descendants 范围；容器和目标均要求唯一。
- 共用原子命令目标解析；find --target 返回稳定查询，不返回临时 ref。
- flow 在 run/before/verify 以及 element: 后置条件处预检组合查询；保存计划后重新定位。
- 使用显式 RawView 的 FindAllBuildCache 子级枚举，与 snapshot 的原始树一致。
- 查询失败、属性读取失败和预算超限均保留错误；wait --gone 不将观察失败视为消失。
- 组合查询不使用旧的模糊、序号、距离锚点和 ItemContainer 回退，不增加依赖或 async。

## 自动检查

单测覆盖条件组合、字面量与大小写、严格 JSON、容器不存在/不唯一、直接子级/后代、
容器自身排除、重建后重定位、枚举失败、遍历深度限制、等待错误传播。
契约 golden 覆盖参数错误 envelope、组合流程预检、歧义候选。
CLI 测试覆盖各元素目标入口与正文不误解析；流程测试覆盖预检失败不创建运行、暂停启动保存计划。

本轮门禁首轮在新增代码的 Clippy collapsible_match 检查失败，修正后通过；
最终窗口解析和原始树修正后 `cargo xtask gate` 通过：205 项 Rust 测试通过、4 项按原配置忽略，
10 个 pack 校验通过，回归框架检查通过。最终日志：`target/uia-composite-final-gate.log`。

## 只读真机复现与修正

环境：Windows 10.0.26200.0，现有系统任务栏 Shell_TrayWnd。
只观察任务栏，没有启动/关闭应用，没有输入、点击或修改急停标志。

首次 snapshot 可以读取 AutomationId=40965，但组合查询返回 NOT_FOUND；重复 snapshot 仍能读取该节点。
独立复现脚本 [uia-composite-observe.ps1](../scenarios/uia-composite-observe.ps1) 在修正前复现失败。

- 失败证据：`target/uia-composite-observe-fd447477ff354ced9822d4fdcae38004/report.json`。
- 被测二进制：`target/debug/actl.exe`，SHA256
  `44DBD6F13BFBE2C89DF688654E4099D451D4AA13EA1BBA6D927926DCD0CAE6C1`。
- 初始路径用普通 FindAll；改为 FindAllBuildCache 并显式配置 true TreeFilter 后，同一节点可定位，
  children 范围也与 snapshot 深度一致。该差异通过本机前后实测确认。
- 修正后调试版报告：`target/uia-composite-observe-db9a2f1db73d474881f50c40cf832f37/report.json`，
  六项通过；SHA256 `909636F507027BC38073AEE31C2740EDB28639EB151D80AF6E3AA1C6CBA4CD0E`。
- 原失败记录不覆盖；所有记录保留每次调用参数、envelope、stderr、环境、哈希和无清理动作说明。
- 最终发布版 `target/validation/release/actl.exe` 六项复测通过，报告：
  `target/uia-composite-observe-ecd45250243143b29f2a500b9395ffd5/report.json`；
  SHA256 `3E0DA45EF02753274EAB564C48FF5B9749E991A6078E760132D60EB89B98A882`。

实现参考微软 [FindAll](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationelement-findall)
及 [FindAllBuildCache](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationelement-findallbuildcache)。
桌面窗口只枚举直接子级，避免把整个桌面作为后代搜索范围。

复测命令（先用 list-windows/snapshot 查当前 HWND 和实际存在的 AutomationId，不复用这里的临时句柄）：

```powershell
pwsh -NoProfile -File scenarios/uia-composite-observe.ps1 -Hwnd <observed-hwnd> -AutomationId <observed-id>
```

## 尚未验收与边界

任务栏只读测试不等于计算器/记事本写入、UIA provider 故障注入、桌面交接或 flow 写入中断恢复验收。
这些场景继续留在 ROADMAP。动态界面在枚举期间仍可能变化；当前查询不提供 UI 树事务快照。
不承诺支持未物化控件，不自动展开控件；COM 单次调用仍可能阻塞，轮询暂停不能强行打断它。
