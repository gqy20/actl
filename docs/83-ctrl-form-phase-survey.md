# 83 · 控件形态与 actl 缺口评估(VMware Workstation / Eval Center 实战轮)

日期:2026-10-13 · 性质:调研评估,作为后续 spec 提交的决策依据 · 关联:docs/78(Broadcom + VMware 实战)、apps/{browser,vmware}.yml

本轮实战 30+ 次控件交互,样本覆盖浏览器表单、VMware Workstation 向导、VMware 硬件设置对话框、Windows 安装程序。盘点控件形态分布、actl 既有通道与缺口,按跨软件通用性排序,定出 Phase 1/2/3 的实施边界。

## 一、控件形态频次与既有通道

| 形态 | 本轮频次 | 跨软件通用 | 既有通道 | 状态 |
|---|---|---|---|---|
| 文本输入框(`Edit`) | 15 | 极高 | `set-value` / `type` | OK |
| 选择框(`RadioButton`) | 5 | 高 | `click Invoke` | **读回无**——只能截图确认 |
| 复选框(`CheckBox`) | 3 | 高 | `click Toggle` | **读回无** |
| 组合下拉框(`ComboBox`) | 5 | 中高 | `select` 已通(浏览器 `<select>`) | **Win32 ComboBox 提交通道缺**(type+tab 才能生效) |
| 文件对话框 `#32770` | 4 | 极高 | 全套已通(见 docs/15) | OK |
| 自绘控件(卡片/菜单/工具栏) | 5 | 中(自绘普遍) | find-image + click xy: --physical 兜底 | OK;**`Invoke成功 ≠ 视觉生效`假成功**未被识别 |
| owned 模态弹窗 | 4 | 极高 | `list-windows` | **owned popup 不在顶层窗口枚举中**(无独立 hwnd) |
| 凭据类输入框(密码) | 2 | 低频高风险 | set-value | **读回恒空边界**(docs/06 §5.2 已记) |
| 嵌套向导页面(横向翻页) | 8+ | 极高 | 全套 OK | OK |

## 二、三大错判来源(本轮实证)

### 1. RadioButton/CheckBox 无读回通道

向导"Typical/Custom"、`Only the files needed to support a virtual TPM` 选中、Power on checkbox 状态,actl **只能 click,不能 verify**。每选一项都截图肉眼确认,曾误判"成功" 6 次以上。

**修法**:`get` 属性面补 `selected`(RadioButton,SelectionItem.IsSelected)/ `checked`(CheckBox,Toggle.ToggleState)。snapshot 现有 `value` 字段语义顺延:TextPattern/ValuePattern 字段名为该模式提供的"当前值",本字段为"是否选中"。这两个通道大多数目标已支持(实际读起来几乎不走 ReadBackEmpty 边界)。

### 2. verify 三态混淆

`set-value --verify-value` 报错时把"通道不支持读回"和"读回不匹配"混在 `ASSERTION_FAILED` 下:

| 实际 | actl 现在报 | 我误判 |
|---|---|---|
| 写入成功但控件 ValuePattern 不可读(密码框、安全编辑框) | `ASSERTION_FAILED` | "没写进去" |
| 写入并真生效、读回可读但内容不等 | `ASSERTION_FAILED` | "写入异常"(多绕 4 步) |
| 读回真的为空 = 通道不支持读回 | (没人报——assertion 自身失效)| (被错误码当"未匹配") |

**修法**:`error.evidence.readback` 增三态字段:

```
"readback": "verified"        // 写后读回与写入一致
"readback": "empty"           // 读回恒空(通道不支持/安全编辑) — 诚实降级,hint 指向 screenshot 核验
"readback": "mismatch"        // 读回可读但与写入不等 — 真失败
```

`readback=empty` 时不报 `ASSERTION_FAILED` 而报 `NOT_ACTIONABLE`(reason=readback_empty,hint 指向截图核验);`readback=verified` 才走 `ASSERTION_FAILED` 路径。`readback` 字段缺省时为 `n/a`(默认 get 路径不读回)。

### 3. 自绘控件"假成功"

Home 页 Create/Open 卡片 Invoke 返回 `ok:true` 但画面零变化,只有 find-image 物理点击才生效。当前 `Invoke 返回 ok` 等同于"已执行",没有视觉核验通道。本轮每次都靠截图兜底——可行但低效。

**暂不修**:`find-image` 模板制作用 PowerShell .NET 拼接可行(本轮实战),真实需求密度上来前不值得投入通用视觉动作层。

## 三、Phase 1(立即做):控件语义层补强

三个改动打包成一个 spec 提交(按硬规则 8 配 golden + docs/06 + CHANGELOG):

1. `get` 属性面补 `selected`/`checked`——消除向导/单选/复选的"必须截图确认"路径;
2. `error.evidence.readback` 三态——密码框类边界不再伪装失败;
3. docs/06 §3.7 同步 get 语义与"readback empty 报 NOT_ACTIONABLE 不是 ASSERTION_FAILED"契约,§4 错误表 NOT_FOUND/NOT_ACTIONABLE 行追加 readback 形态字段含义。

预计改动 30-50 行,跨 2 crate(actl-uia 读 selected/checked、actl-core + actl-cli 改 error evidence),配 1-2 golden(verify 三态、selected/checked)。

## 四、Phase 2(视真实需求再做)

- **Win32 ComboBox type+tab 提交**:Win32 传统应用 VMware/Office 类普遍,本轮 CPU 核数掉坑重做 5 步——有真实需求。补一条新通道 `combo-set` 或 `submit-value`;
- **owned popup 窗口枚举**:`list-windows` 增 `--include-owned` 选项,枚举 #32770 owned 弹窗——文件对话框失败处置收益大,但 4 次实战中只有 1 次卡在 owned 弹窗(密码错误框),优先级中等;
- **find-image 模板制作辅助**:`actl crop --element id:X --app W --out template.png`——从元素 rect 直接落物理 PNG,跳过本轮 PowerShell 拼接脚本的 4 行 Python 包装。

## 五、Phase 3(观察,暂不投入)

- 自绘控件通用视觉动作层(除卡片外还有 Office Ribbon/某些 Electron 应用)——find-image 兜底已验证有效,在更多真实场景密度起来前不开;
- ComboBox 通过 `Expand` + `Select` SelectionItem 路径走通——`select` 已有但下拉项未物化时返回 NOT_FOUND,该走 `Expand` 模式而非点 ComboBox 顶部。等真遇到再补;
- 颜色/图形通道(用于"高亮蓝色"等样式断言)——本轮用截图就够了,颜色断言增加约 1KB 库,价值未达。

## 六、纪律复盘(非产品缺口,进工作流)

本轮犯的低级错误大半是纪律不是产品:

- **release 二进制上跑 batch 解析 data 失败**:release.exe 是 16:19 旧版,本轮 spec 修复都在 debug。纪律:跑任何协议面断言前确认 `actl --version` ≥ 最新 commit;或养成 `cargo run -p actl-cli -- <cmd>` 跑 debug 的肌肉记忆;
- **STALE_REF 反复出现**:ref 跨操作要重取,旧 snapshot_id 不能复用。纪律:每次 click/set-value/press 之前若大于 N 步间隔,重取 ref 一次;
- **password 框 set-value 后读回失败的误判**:这条已被 Phase 1 修复消除,但归档在此作为评测依据。

纪律项进 pack/工作流,不是 spec。

## 七、与 docs/78 的关系

docs/78 是 L4 真实标本(预填禁用 + SPA 重渲染 + companion 版本门控),本轮实证是它**直接衍生的 Phase 1/2 评估**——L4 陷阱题产出契约缺陷,契约缺陷评估后入 spec。三者关系:

- docs/78 §5 L4 陷阱题:产生 bug 样本的题目
- docs/83(本文件):样本背后的能力缺口与 spec 边界
- 后续 spec PR:实现 docs/83 Phase 1 的三个契约点
