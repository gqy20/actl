# 容器局部快照

日期：2026-09-29。协议见 [06](06-design-blueprint.md)，待办见 [ROADMAP](ROADMAP.md)。

## 本批范围

已知唯一容器时，用 `snapshot --app <window> --within 'uia:{"match":{"id":"panel"}}' --depth 3 --limit 100`
观察容器及其后代，再按需要叠加 skeleton/content 投影。根深度为 0，节点限额包含根。
原有全窗口 snapshot 行为保留；局部快照输出范围、投影前节点数及真实截断原因。

局部编号独立从 @e1 开始，后续必须绑定返回的 snapshot_id。磁盘记录保存范围和预算，
重放按原范围枚举并核验窗口、容器和元素身份；容器变化不回退到全窗口编号。
未绑定 snapshot 的旧式 ref 无法识别局部范围，仍是全窗口序号，调用方不能混用。

## 验证记录

- 离线契约：缺少窗口绑定的 golden、严格选择器和预算拒绝、help 四要素。
- 原生自有窗口：验证局部 @e1 与全窗口同号引用的区别、绑定后的读取、深度/节点截断、
  已记录元素在截断范围内重放、容器改名后的 STALE_REF、歧义拒绝和窗口清理。
- 测试只读取自有目标，通过窗口 API 改变自有控件标签，不注入鼠标键盘、不请求桌面授权。
- 2026-09-29 Windows 本机 release CLI 首轮原生实测通过：8 次调用，预期失败为 STALE_REF 和 AMBIGUOUS。
  自有 HWND 12652826 已销毁，前台保持不变；逐调用证据在 `target/local-snapshot-live/run-16112/`，
  执行日志 `target/local-snapshot-live.log`，环境头 `target/local-snapshot-environment.txt`。
  被测路径 `E:\explore\computer_cli\target\release\actl.exe`，SHA-256：
  `50F927662768764E4A087D2A267338DD4B05853AF10D26DE289F65C2FE351FF5`。
- 小窗口基线完整输出 556 字节，局部输出 572 字节：节点减少，但范围元数据带来额外成本。
  不把此轮记为 token/字节削减验收。新增成功 envelope golden 后再次执行原生测试，仍通过；
  证据 `target/local-snapshot-live/run-52180/` 和 `target/local-snapshot-live-golden.log`，清理与前台核验通过。
- `cargo xtask gate` 通过：307 项通过、25 项按交互桌面要求跳过；包含格式、clippy、契约、
  pack lint、回归脚本和打包检查。日志 `target/local-snapshot-gate.log`；成功 golden 补齐后最终门禁
  同样通过，见 `target/local-snapshot-gate-final.log`。
  日志中的隔离 Stop active 是回归脚本主动构造的拒绝用例，不代表当前用户的停止状态。
- 已将同哈希 CLI 更新至 `C:\Users\gqy17\.cargo\bin\actl.exe`，旧文件备份到
  `target/local-snapshot-installed-backup.exe`；安装后 help、execution-status 和 display-status 核验通过，
  执行状态 ready、stop=null，显示端 PID 65836 就绪。

## 限制与后续验收

容器唯一性定位仍可能扫描整个窗口，不能据输出减少宣称总 UIA 扫描耗时下降。
小窗口中额外范围元数据甚至可能增加 envelope 字节数；字节数也不等于模型 token 数。
真实软件的大容器、虚拟化列表、控件重建、provider 故障、外部 UIA 对照尚待覆盖。
本批不自动改写应用 YAML，应用档案只同步经过真实软件验证的定位知识。
