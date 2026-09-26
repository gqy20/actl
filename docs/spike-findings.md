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
