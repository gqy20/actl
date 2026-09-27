# 07 · 金任务集(验收标的)

各 Phase 的退出标准以本文件任务为准(ROADMAP 引用此处,不重复定义)。
所有任务在**干净启动**的目标应用上执行,任务间恢复初始状态;成功标准 = N 次重跑全部通过。

## 金任务 1 · 计算器(M1 主验收,10/10)

2026-09-27 迁移为 PowerShell：激活商店包中的计算器，以新 HWND、程序路径和启动时间确认归属，
每轮 snapshot 后依次用 ref 点击，并用 verify 检查结果名称。默认 3 轮；正式 10 轮验收设置
`$env:RUNS='10'` 后运行 `cargo xtask regress --case golden-task-1`。历史 10/10 不代表新脚本
通过；授权解除停止后已完成 10/10 轮计算断言，清理误判已修正并单独验证。
最终脚本的整轮重跑仍待完成，具体证据和边界见 [测试基建](22-adaptation-test-infrastructure.md)。

```
1. actl launch calc
2. actl snapshot --app "计算器"
3. 通过 ref 依次 click:1 2 3 × 4 5 6 + 7 8 9 =
4. actl get <结果控件> --property value
断言:读出 "56,877"(123×456+789)
```

覆盖能力:launch / snapshot(ref 编号) / click(UIA Invoke) / get / 中文控件名。

## 金任务 2 · 记事本写入保存(M1 主验收,10/10)

2026-09-27 测试入口迁移：历史 10/10 记录不代表新脚本验收。当前用例打开本轮独立空文件，
写入中英文后以 Ctrl+Shift+S 另存到绝对路径，核对完整磁盘内容。已有记事本会话或紧急停止时
返回 blocked；只关闭本轮已保存的测试标签。运行 `cargo xtask regress --case golden-task-2`，
详细边界及迁移状态见 [测试基建](22-adaptation-test-infrastructure.md)。

```
1. actl launch notepad
2. actl type <编辑区> "hello actl\n第二行中文"
3. actl press ctrl+s --expect window:"另存为"
4. 在另存为对话框输入路径,确认保存
5. (外部)校验文件内容逐字节一致
```

覆盖能力:launch / type(键盘路径+中文 IME 语义) / press / wait(--expect 内联验证) / 对话框交互。

## 性能指标(M1 退出时记录,方法见 PRD §4)

冷启动 P95:连续 100 次 `actl status` 采样(排除首次);金任务 1 全程 wall-clock 记录。

## 金任务 3 · 跨 3 应用数据搬运(M2 主验收,10/10)

从老 Win32 应用(候选:任务管理器进程页/系统信息 msinfo32)extract 结构化数据 →
计算器运算 → 记事本写入结果并保存。
M2 初期细化为可脚本化验收;验证 extract/set-value/wait 谓词/fallback 定位链。

## 测试纪律

- 断言只依赖系统自带应用(计算器/记事本/mspaint/ms-settings),不引入外部安装依赖;
- 失败一次即中止计数并记录 evidence(局部树 + 复现步骤),修复后重新起算;
- 金任务脚本进 `fixtures/`(M1 末开始),先手工执行、后脚本化。
