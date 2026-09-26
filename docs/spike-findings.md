# M0 Spike 坑清单(UIA 五场景实测)

日期:2026-09-26 · 环境:Windows 11(10.0.26200)/ 工具链 1.98.1-msvc / uiautomation 0.25.1(dev-dep)
工具:`cargo run -p actl-uia --example probe -- windows|focused|tree <target> [depth]`

## 结论速览

| # | 场景 | 结论 | 影响 |
|---|---|---|---|
| ① | Electron/Chromium(ZCode 实测) | **内容可达但深而稀疏**:aria 桥接工作正常(Button/Text/Tree/automation_id 如 `sidebar`、`radix-*` 均暴露);但真实内容在 depth ≥15,depth≤10 只有 15 元素且 11 个空名 Pane;总样本 63 元素中 45 个空名 Group | snapshot 必须配骨架投影;可交互判定**不能依赖有名称**;深树遍历需性能预算 |
| ② | 管理员窗口 / UIPI | **未触发**(本会话无 elevated 窗口,枚举 20 窗口全部可读) | 探测方法已内置(windows 模式的 UNREADABLE 标记);待有 elevated 环境复测,PERM_DENIED 的触发条件暂以文档假设为准 |
| ③ | 中文 UIA 树 | **完美**:任务栏/资源管理器/锁屏等中文 Name 原生 String 读取无乱码;`还原/最大化/关闭` 等系统控件名均为中文 | 编码无坑;中文 Name 可直接作为选择器值 |
| ④ | Canvas(mspaint 实测) | **分化**:新版画图 ribbon 极丰富(221 元素:60 Button/53 ListItem/46 Image,全部可定位);但**画布像素区仅一个无名 Custom**,内容完全不可达 | 证实 `--physical` 兜底与 ADR-007 分层设计必要;工具栏类操作 L1 可全覆盖 |
| ⑤ | RDP 会话内 | **未测**(本机无活动会话) | 留待环境;低风险(UIA 在 RDP 会话内通常正常,坑多在断开会话后的会话 0) |

## 重要发现细节(M1 设计输入)

1. **Explorer 内容区疑似浅截断**:对资源管理器主窗口 walker 遍历只得到 TitleBar 层(depth 3/8 元素),内容区(DirectUIHWND)未出现——M1 实现 snapshot 时需对 Explorer 专项复查(可能与匹配到了错误窗口或内容在兄弟分支有关)。
2. **焦点元素可能落在后台 UI**:focused 模式读到的是"锁屏界面"Pane(pid 39856)而非前台应用——`get_focused_element` 语义是"系统焦点链",M1 的 `status`/焦点相关命令要考虑这一点。
3. **性能量级**:221 元素树遍历(debug 构建、含 ~0.5s 进程冷启动)共 0.9s,纯遍历约 0.4s(debug)。M1 的 snapshot 目标(<500ms,release)可行但**必须使用 cache request**(crate 提供 `get_*_build_cache` / `UICacheRequest`)或按需属性读取,禁止逐元素逐属性跨进程 COM 往返。
4. **walker 尽头语义坑**:uiautomation crate 的 `get_first_child/get_next_sibling` 在"无子/无兄弟"时返回 `Err`——与真错误不可区分。生产代码必须做 HRESULT 分类(windows crate 层),spike 中以 `.ok()` 视作尽头仅限探测用途。
5. **空名元素是常态而非异常**:63 元素样本 71% 空名(Chromium),mspaint 也有 29%。骨架投影与 `find` 的谓词设计按"名称可选"处理。

## uiautomation crate 评估结论(TECH_STACK §3 的 M0 决策)

**M1 采用,升为 actl-uia 正式依赖。** 依据:五场景核心路径(连接/遍历/属性/焦点/类型)全部工作;API 类型安全、面窄(walker + get_*);文档齐全;错误统一 `Result`。附带上文第 4 条的错误分类义务。备选(自写薄封装)仅在遇到 patterns/events 层缺陷时启动。

## 遗留(不阻塞 M1 开工)

- ② UIPI:elevated 窗口复测(需要一次管理员环境);
- ⑤ RDP:会话内行为;
- Explorer 内容区遍历复查(随 M1 snapshot 实现顺带)。

## 事故记录 #1(2026-09-26,严重度:高,已立永久护栏)

**事件**:press 命令开发期间,`ctrl+a` + `delete` 测试组合在焦点判定失效时被注入到开发者的真实前台窗口(资源管理器),触发全选删除;用户手动拦截,未造成实际丢失。

**根因链**(三条叠加,每条单独都不致命):
1. uiautomation crate 的 VK 注入 wScan=0,WinUI 目标丢弃按键 → 键"看似消失";
2. UIA 焦点视图(get_focused_element)显示目标窗口,但**物理键盘前台(GetForegroundWindow)不一致**——验证方法用了会说谎的信源;
3. SendInput 不会消失:没进目标的键全部进了当时的物理前台(用户的真实窗口)。

**永久护栏(已实现,press --app 路径强制)**:**verify-then-inject**——注入前以 Win32 物理前台为唯一判据,前台不匹配目标窗口则 PERM_DENIED 拒绝发出任何键。宁可失败,不可错发。护栏上线首日即拦截一次真实偏射。

**纪律**(AGENTS.md 安全红线):全局键盘/鼠标注入的任何验证,必须在受控靶窗口 + 物理前台验证通过后进行;禁止以"目标窗口没反应"作为唯一判据(键可能打进了别处)。

## 键盘注入排查战记(2026-09-26 深夜,金任务 2 攻坚)

一晚从"按键神秘消失"到"金任务 2 全通"(type → ctrl+s → 另存为 → set-value → 磁盘验证,7/7)。
方法转折点:用户指示 **"调研最优开源实现 + 截图核对",停止瞎猜**——两个动作分别终结了理论循环和观测循环。

### 四重假信号(每个都骗过了我至少一轮)

1. **窗口标题(tab 名)≠ 编辑器内容**:新记事本 tab 名只取首行且滞后更新;"标题没变"被误读为"没输入"。真相:整晚注入都在工作,编辑区里早躺着所有"消失"的字符(截图转写逐字对出 `sabbz` = 历次 press 的 a/b/b/z/z)。
2. **设置页打开时窗口标题仍是文档名**:标题显示 `*helloactl...` 不代表在文档视图;设置视图会静默吞掉一切快捷键(包括 ctrl+s)。判据必须是 UIA 树里有没有 `Document` 元素。
3. **UIA Name ≠ Win32 标题**:"另存为"对话框 GetWindowTextW 可读,但 UIA 枚举 root 直接子节点里没有它(见下,owned dialog)。
4. **"ok:true" ≠ 效果达成**:SendInput 全部送达(SendInput 不会失败),但目标把 UNICODE 's' 当文本打进了文档——注入成功与语义达成是两回事。

### 三个真根因(按发现顺序)

1. **UNICODE 包不吃修饰键**:`KEYEVENTF_UNICODE` 走 WM_CHAR 文本通道,修饰键 VK 状态对它无效 → `ctrl+s` 退化成裸 `s`、`ctrl+a` 退化成裸 `a`(文档里那些孤立字符全是它们)。修复:组合键成员经 `VkKeyScanW` 映射为 VK 事件(AHK/pywinauto 同款,字符优先 VK、映射不出才 UNICODE)。
2. **VK 通道字符会进 IME 组合**:组合键修复后纯文本走 VK,中文输入法把空格当"上屏确认"吞掉、半角标点全角化(`golden task 2:` → `goldentask2:`)。修复:**双通道规则**——带修饰键 → VK(IME 不拦 ctrl+键);无修饰键 → UNICODE(绕 IME)。
3. **修饰键用左右手变体**:VK_LCONTROL 的 DOWN 被 WinUI 输入栈丢弃(AHK 源码注释:"左右变体 aren't fully API-supported";pywinauto `'^'`→VK_CONTROL)。修复:通用 VK_CONTROL/VK_MENU/VK_SHIFT,清场发全集 11 个(通用+左右)。

### owned dialog(IFileSaveDialog)不可见问题

"另存为"对话框不是 UIA root 的直接子节点,而是**挂靠宿主窗口的 depth-1 Window 元素**——find_window / list-windows / --expect 轮询全部漏掉它,而物理前台(GetForegroundWindow)明明是它。修复:`scan_top_window` 同时扫 root 子节点 + 各顶层窗口的直接子 Window;list-windows 同步补列。

### @eN ref 漂移(跨调用)

ref 编号绑定"遍历顺序",两次调用之间树一变(如对话框内列表刷新),`@e99` 就从 Edit 漂成 Button——脚本层静默点错元素。同型事故:`click @e1` 未带 `--app`,在快照与点击之间前台切换,静默点进了别的应用(ChatGPT 的"快捷设置")。修复(fail-closed):**@eN 未指定 --app 直接 PROTOCOL 拒绝**;跨步骤定位优先语义选择器(`id:1001` 文件名框全程稳定)。

### 沉淀为机制的决定

- **内容读回验证**:效果断言一律 UIA Value 读回(或磁盘终验),禁止用标题当判据(fixtures/golden-task-2.sh 第 3/7 步)。
- **截图核对**为最后仲裁手段(本次靠它终结了 ①② 两个假信号循环)。
- `type` 命令补齐 verify-then-inject 护栏(此前只有 press 有——同类洞不修第二个)。
