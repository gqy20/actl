# Changelog

本项目的全部显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),
版本语义遵循 SemVer;0.x 期间破坏性变更在条目中以 **[BC]** 标注。

## [Unreleased]

### Added
- **朋友圈实测全流程(用户授权) + `click --physical` rect-center 兜底(产品改进)**:
  ① mmui 导航图标不暴露 clickable point,旧实现直接拒——现受控退化为
  **BoundingRectangle 中心 + 命中检查把关**(命中检查不过仍拒,fail-closed 保留;
  实战首例:ZCode 遮挡时正确拒绝误点);② 朋友圈闭环:发现(物理点击)→ 快照取
  发现页首项 ref 精点 → wait id:sns_list 物化 → 快照 name 解析帖子(作者+来源+
  正文+时间复合文本,实测两条逐字)→ scroll id:sns_list 物理滚轮(**≥15 档**,
  虚拟化懒加载阈值,5 档无效果)→ 新帖出现;③ 恢复:click name:微信
  **--near name:通讯录** 锚点消歧(与窗口同名冲突的实战解法)。pack 增
  moments/moments_back 路线 + quirks;lint 37 步全绿。
- **微信 pack 真机修正(打开/点击/输入实测,未发送)**:① 会话条目 UIA Invoke
  返回 ok 但**不触发导航**(mmui/Qt 特性)——切换会话必须 `--physical`(实测
  物理点击后消息区即时物化);② 消息气泡是 Custom role,extract 不命中——读消息
  走快照 name 解析(气泡 name=正文全文,虚拟化仅含可见段);③ 输入闭环全通:
  set-clipboard → click 输入框 → paste → `get --property value` **读回逐字**
  (输入框暴露 ValuePattern);④ 清草稿 = ctrl+a + delete(回车即发送,严禁);
  会话条目 name 为"名称\n置顶\n预览\n时间"复合文本。pack 路线重写为
  session_scan/open(physical)/read(snapshot)/draft(读回)/clear_draft/send(高危,
  仅核对后,未真机验证)。
- **微信 pack(skills/actl/apps/wechat.yml)**:知识来源 = 同作者的 wechat-ui-collector
  项目(weixin-4x profile:mmui:: 选择器体系)+ 本机只读真机验证(session_list 可达、
  session_item_<名称> 富 id 定位会话、消息区随打开物化)。路线:会话概览/打开会话/
  读消息(虚拟化 RecyclerListView)/发送(高危标注:核对标题+读回确认+严禁盲发);
  quirks 六条含后台操作参考(LASTINPUTINFO 空闲检测 + WS_EX_NOACTIVATE 后台点击,
  actl 后台化列 M3 候选)。lint-packs 3 packs/28 步全绿。
- **`paste` 命令(spec,新原语)**:ctrl+v 到目标窗口**当前焦点/选中处**——无元素
  落点(Excel 选中格/文件管理器选中项/重命名框)的粘贴原语。刻意不做元素级聚焦
  (SetFocus 会移动选中);护栏链 = 窗口前置 + 物理前台核对 + 输入锁;
  `input_effects.clipboard: "consumed"`。真机:Excel F2 写入闭环五步缩四步,
  读回逐字。
- **`launch` App Paths 解析 + stdio 分离(通用修复)**:裸程序名(Winword/Office
  别名)不在 PATH 时查注册表 App Paths 取完整路径(实测 `launch WINWORD` 68ms
  解析启动);GUI 子进程 stdio 显式分离——继承管道句柄会让调用方(head/$())永远
  等不到 EOF(实测挂死根因)。
- **应用档案 pack v1(skills/actl/apps/*.yml)**:路线知识从 docs/14 固化为可校验
  数据(agent 消费,核心 CLI 保持原子不读它)。excel/word 两档:cell_write/read、
  table_extract、body_read/append、save_inplace 路线 + 受保护视图等 quirks;
  `cargo xtask lint-packs` 零依赖校验(结构四键 + 17 步全对 `actl commands`
  schema——命令全集单一事实来源,防漂移)。SKILL.md 增"应用路线"节。
- **Office 保存链 + Word 写入闭环(解锁后续测,docs/14 收口)**:真实场景 =
  打开已有文件 → 修改 → ctrl+s **原地保存**(不弹窗——断言走磁盘终验而非
  --expect window,此前对草稿 Book1 等弹窗是场景构造错误,已纠正)。Excel:
  剪贴板直灌 E1 → ctrl+s → COM GetActiveObject 终验 E1="saved by actl" + mtime
  更新;Word:type --paste 追加(delivered:14)→ ctrl+s → TextPattern 读回原文
  完整 + COM 磁盘终验双中。**受保护视图发现**:TEMP 等不可信位置文件,
  Excel/Word 静默拒绝编辑(ctrl+v ok 不落格、保存命令沉默)——安全特性正确
  工作,actl 不绕过,正解 `click name:启用编辑`;草稿 Book1 上 F12/ctrl+s
  沉默的同族归因。三个闭环(读/写/保存)双应用全通。
- **Office 场景实测(docs/14,真实需求最大空白填补)**:Excel——有数据的网格物化为
  DataItem(1246)+DataGrid,**`extract --table` 的 grid-pattern 直读首次真实触发**
  (43×28 全网格,行列标齐全、数据逐格正确);字段填写闭环 = set-clipboard →
  名称框(id:1001)跳转 → ctrl+v 直灌 → ctrl+c + get-clipboard 读回(实测逐字)。
  Word——`get --property text`(TextPattern)中英文正文逐字命中(Chromium 之外
  第二个实证应用)。保存链/Word 写入因实测中桌面锁屏待续测(护栏正确拒绝锁屏注入,
  UIA 后台调用不受影响——重要特性发现)。产品修复:ensure_element_focus 前置免扰
  (焦点已在目标窗口时不再强行 SetFocus 首个 Edit;Excel 多 Edit 场景旧启发式会
  把正确焦点搅到搜索框,后续按键全被吞)。
- **batch 步间弹窗闸门(blocked_by,warning 不阻断)**:每步前采样前台窗口,若它既不在
  批前基线、也不匹配本批任何 --app 目标 → 该步 steps 条目附 `blocked_by:{title,hwnd}`;
  失败时刻补采样一次(launch 竞态/慢弹窗在步执行中才成形)。语义 = **前台劫持检测**
  (更新提示/对话框抢焦点——注入类步骤的真正威胁);不抢前台的新窗由 windows_created
  兜底,两机制分工。真机实证:批执行中弹 Updater 对话框抢焦点 → 失败步 evidence 直接
  指出 `"blocked_by":{"title":"Updater"}`;
- **弹窗处理协议文档化**(skills/actl SKILL.md):三档分类(宿主弹窗 owner_title 判定/
  消息框按按钮名点/意外弹窗读 blocked_by 处理后重试)+ windows_created 收尾清理协议
  (close-window --pid/--hwnd,禁 taskkill)——agent 可直接执行。
- **batch 窗口差集上报(spec,加法)**:进入时取窗口 hwnd 基线,结束时差集 = 本轮新开窗口,
  非空时 data 与失败 evidence 携带 `windows_created: [{title,pid,hwnd}]`——生命周期仍归
  编排层(原子 CLI 不代管、不悄悄关窗),清理地基闭环(list-windows 发现 → close-window
  --pid/--hwnd 清除);
- **fixtures 失败安全清理改造(窗口治理)**:全部 live fixture 换 trap EXIT 清理(成败/
  中断必执行;hwnd 精确优雅关闭 + "不保存"对话框处理)、前置断言(环境脏即 ENV-DIRTY
  fail-fast 并顺手清理)、**全面弃用 taskkill**(GUI 会话复活的根源:记事本 TabState+
  云同步、计算器僵尸框架)。金任务 1 顺带自启化(不再依赖预开计算器);timing 两 fixture
  的 taskkill 一并清除。实战:regress 六 fixture 全绿后**窗口/临时目录零残留**,
  US1 16/16 恢复(此前环境阻塞)。
- **一键回归入口**:`cargo xtask regress` / `make regress`——release 构建 + 顺序跑全部六个
  live fixtures(金任务 1/2、US1、验收 A/B/C)并汇总。Windows 陷阱实录:裸 "bash"
  解析到 System32 的 WSL bash(fixture 全线假败,报 WSL_E_DEFAULT_DISTRO_NOT_FOUND)——
  显式探测 Git Bash 路径,`ACTL_BASH` 可覆盖;
- **close/focus-window 按 --pid/--hwnd(FR3 补缺)**:窗口命令三形态定位(标题子串/
  进程/句柄,clap 互斥);PID 多窗口 → AMBIGUOUS 附候选标题(fail-closed,同标题多窗
  的正确行为);`list-windows` 每窗口补 `hwnd`(十进制)。真机闭环:notepad 双实例
  --pid 被 AMBIGUOUS 正确拒绝、--hwnd 精确关闭其一;
- **actl-signal Per-Monitor DPI**:WM_DPICHANGED 处理(DPI 基准更新 + 字体缓存失效 +
  采纳系统建议矩形)+ 启动时按窗口落点 DPI 校正(DPI 基准从 OnceLock 改 AtomicU32)。
  本机验证:渲染清晰正常;跨屏拖动的完整行为待手动拖动复测(本机处于系统 DPI 虚拟化
  场景,窗口逻辑坐标与物理坐标不同源,如实记录)。

### 回归(清账批)
- `cargo xtask regress` 首跑:金任务 1 3/3、金任务 2 10/10、验收 A 8/8、B 9/9、
  C 8/8;**US1 环境阻塞**:Win11 记事本云会话恢复自动重建历史窗口 → 多窗 AMBIGUOUS
  (行为正确,环境脏了)——taskkill 会复活(TabState),本地 TabState 清除不够(云同步),
  需一次性手动关闭记事本"自动保存会话"设置后复跑;记事本全链已由金任务 2 10/10 覆盖。
M2 收官批:命令面补齐(wait 谓词/select/toggle)+ skeleton 单链折叠 + P50 结构优化 + 全命令 help 四要素。

### Added
- **`wait --property` 谓词等待(spec)**:`wait --element <t> --property <prop>=<expected>`
  (prop ∈ value|name|checked,checked 期望 on/off;`--contains` 子串语义)。谓词隐含存在性——
  元素未解析继续等;value 走 Value/TextPattern 双通道。WaitPredicate 解析/判定纯函数,
  3 项单测 + 契约测试(PROTOCOL 路径离线可测);真机:计算器结果断言 `name=56 --contains`
  与精确 `name=七` 双绿。
- **`select`/`toggle` 命令**(蓝图 31 补齐 26/31):select = 容器内按选项名
  SelectionItem 选中(精确→子串兜底,多命中 AMBIGUOUS 附候选);toggle = Toggle 翻转并
  上报翻转后状态(与 click 的分工:无点击副作用/可读状态)。pattern 调用与 click 既有
  分发同路径,真机验证留自然场景。
- **skeleton 单链折叠投影**:非交互保留节点若只剩一个保留子则折叠(倒序单遍收敛,
  窗口根豁免; 编号不受影响——ref 只编号交互元素且 DFS 序不变)。Edge 实测
  33%→36% 字节削减——浏览器形态的剩余体量是工具栏交互按钮本体(结构性不可压),
  据此修订 PRD §4 NFR 为分形态口径(深单链 ≥70% 保留,浏览器/小窗口如实记录)。
- **全命令 help 四要素补齐(06 §3.5)**:29 个命令的 -h 统一 Usage/参数表/
  Example/Errors(表驱动 after_help,clap derive variant 级属性不传播故 builder 注入,
  `build_cli()` 单点);29 命令审计脚本 0 缺失。
- **P50 结构优化**:瞬时读类命令(status/commands/list-windows/get-clipboard)跳过
  begin 态写入(250ms 轮询必然错过的瞬时态,却占冷启动路径;终态写保留)。同环境对照:
  单次 state 写 ≈11ms(慢日值);绝对收益待正常日复测(慢日纪律:绝对值不可跨日)。

### 搁置
- `list-notifications`:WinRT(Windows.UI.Notifications)依赖超出纯 Win32 边界且
  真实通知验收难造——裁定移出 M2 命令面收官,登记 M3 候选(蓝图 FR4 留位)。

## [0.1.3] - 2026-09-27

M2 第三批:观察层强化(虚拟化定位/滚入视野/正文通道/表格直读/窗口叠放事实)+ 定位性能结构优化 + 可观测性(batch steps / ACTL_TRACE)+ 信号条重设计 + CLI 模块化拆分;版本口径修订。

### Added
- **L4 虚拟化容器定位(spec)**:定位链新增第四级——L1/L2 在已物化树中空手时,询问窗口内
  ItemContainerPattern 容器(`FindItemByProperty` 原生直调,NULL 起查;crate 安全包装不接受
  空元素且零值 UIElement 的 Drop 会 Release(null) 崩溃,故经 `UIElement::as_ref()` 取原生
  接口,unsafe 登记于 actl-uia 豁免点)。**精确匹配,无子串语义**(provider 侧属性查询);
  `Role`/`@eN` 不参与;命中元素不在快照树内 → 无 ref,重放走同一 name/id 查询。
  `resolved_by` 新值 `item-container`(docs/06 §5 同步);`find` 在已物化树 0 命中且查询
  可映射时自动走 L4 并上报 resolved_by。Explorer 实证:61 文件目录(walker 树内 0 命中)
  → 精确查询命中按字母序末位的未物化文件;late-create("刚保存的文件")秒级可见——
  US1 挂账的"Explorer 条目级确认"开放项收口;
- **P1 滚入视野配套**:物理指针动作(click --physical/right-click/double-click/hover/drag/wheel)
  前元素 offscreen → 自动 `ScrollItemPattern::ScrollIntoView`(失败静默,由命中检查如实
  拦截);结果新增 `scrolled_into_view` 字段;drag 的 from/to 两侧都处理;
- **`get --property text`(spec,加法)**:TextPattern 文档正文专用通道,与 `value`(ValuePattern)
  分工——Chromium 的 Document Value=URL 会掩盖正文(实测 Edge 页面 value=file:///...),
  正文只能走 text;`value` 读取链补 TextPattern 兜底(空 Value 时),`value_via` 上报通道;
- **`extract --table` GridPattern 直读优先**:`GetItemAt(row, col)` provider 侧行列寻址
  (带 row/col 语义,虚拟化行可达),读不全如实回退树扁平化;新增 `source` 字段
  (grid-pattern | tree-flatten)。msinfo32 实证:44 行 [名称, 值] 结构化提取,中文内容
  全对(source=tree-flatten,该应用无 Grid;grid-pattern 直读待真实 Grid 控件触发);
- **`list-windows` 物理层叠放事实**:每窗口新增 foreground(GetForegroundWindow 判据,
  与注入护栏同源)/ topmost(WS_EX_TOPMOST"总在最前"弹窗)/ minimized / visible(UWP
  后台挂起 cloak 时 false 属预期)/ z_order(Win32 Z 链位次,0=最顶)/ rect(虚拟桌面
  坐标)/ owner_title(owned dialog 宿主)——agent 可直接回答"谁在最上面/是否被遮挡/
  弹窗挂在谁下面";
- **验收 fixtures 三件**:virtualized-list.sh(Explorer 条目级,8/8)、win32-table-extract.sh
  (msinfo32,9/9,金任务 3"老 Win32 抠数"正宗标的)、edge-deep-tree.sh(Chromium 系,8/8);
  全系统自带应用,CI 安全纪律不破;
- **定位性能结构优化**:`id:` 精确匹配走 provider 侧属性条件(UIA FindAll 单次往返替代
  Rust 侧逐元素三连读;恰一命中直接用,多命中回退全遍历保 AMBIGUOUS 候选语义——另存为
  id:1001 双命中先例);collect_matches 惰性属性读(非命中元素每元素 3 次 COM → 2 次,
  Role 目标 1 次)。本日为环境慢日(单 COM 往返 ~15ms,正常 <1ms,timing 文档既有纪律:
  绝对值不可跨日比较),收益绝对值待正常日 timing fixture 复测;
- 金任务 1 fixture:12 键改单次 batch 执行(@eN ref 重放语义保留,进程冷启动 13 次 → 1 次);
  默认轮数 10 → 3(日常回归;正式 M2 验收 RUNS=10)。
- **batch 每步结果与耗时上报(spec,加法)**:`steps[]`(i/cmd/ok/duration_ms,失败步附
  error.code/message)——编排层的"捕获衔接"变成可观测(哪步慢、哪步开始等待);
  首错即停时已执行步随错误 evidence 带出,不再丢失部分结果(M1 遗留的"batch 无 partial"补齐);
- **ACTL_TRACE=1 引擎阶段打点**:stderr 旁路(`[actl-trace] <stage> <ms>ms`),覆盖
  capture.window/wake/walk、locate.window/wake/fastpath/dfs/l4、click.pattern——慢环境
  把命令总耗时分解归因(启用当日即实证:1.8s 命令中 locate.window 段占 2036ms,fastpath 5ms)。
  stdout 契约不变(ACTL_TRACE 只进 stderr,契约测试锁定);禁用时零输出。
- **actl-signal 交互重设计**(用户反馈驱动:去废话/设计感/呼吸感/持续工作感知):
  ① 提示行删"✕ 退出"(退出在右键菜单,不占提示位);失联行删">10s"(阈值是实现细节);
  ② **条宽自适应内容**(DT_CALCRECT 量三行,280–460 钳制,锚定右缘;右侧不再大段留白);
  ③ **状态灯连续动画**(Codex 式"一直在干活"感知):执行/注入中 = 双波纹扩散
  (ease-out + 向底色淡出,25fps 双 timer,只失效状态灯小区域),空闲 = 慢呼吸(±12%,2.4s);
  停止/失联保持静止(终止态不制造动感);④ **执行中右侧实时耗时**(state.json 时间戳推算,
  0s→59s→1m03s,每秒跳动——变化检测键含 elapsed,非执行态仍零重绘);
  ⑤ WM_PAINT 改读快照缓存(绘制路径零文件 IO)。波纹/呼吸/混色/耗时格式均为纯函数,
  4 项新单测锁定。真机截图核对:空闲(呼吸灰点+收窄条)/执行(wait 主行+绿点波纹+右侧 5s)两态全过。
- **门禁新增单文件行数检查(xtask gate 首步,纯本地最快失败)**:`crates/*/src` 全量 .rs
  逐个计数(整文件,含注释/空行),>800 行警告、>1500 行失败;examples/tests 不扫,
  将来引入生成代码须在 xtask 登记豁免。依据:社区惯例手写 Rust 非测试代码 ~400–800 行/文件
  为宜(serde/clap 等一线 crate 文件中位数 150–220 行,仅个别 2–5k 聚合核心);
  pre-push 与 CI 经 `cargo xtask gate` 自动获得同一检查(门禁单点不变)。

### Fixed
- probe 探测工具补 `ic` 模式(ItemContainer 容器发现 + 原生查询验证,spike 载体)。

### Changed
- **actl-uia/src/lib.rs(2112 行,当时全仓最大文件)按命令域拆为五模块**:
  `window`(窗口发现/capture/快照持久化/窗口管理,668 行)、`locate`(定位链 L1–L4 +
  元素等待,587)、`action`(语义动作 click/type/press/set-value,286)、`pointer`
  (物理指针 + scroll,212)、`read`(get/verify/extract + 剪贴板,380),lib.rs 收敛为
  49 行模块中枢。纯代码搬移无语义改动;公共 API 经 `pub use` 汇聚 crate 根,crate 外
  路径不变;跨模块原私有 helper 按需升 `pub(crate)`。
- **actl-cli/src/main.rs(1376 行)按职责拆为三模块**:`cli`(clap 参数面 + help 文本
  同源 doc comment,373)、`interact`(操作类执行体 + 注入护栏,353)、`query`
  (观察/编排类执行体,421),main.rs 收敛为 269 行入口(状态信号 + dispatch + envelope
  落盘)。envelope 输出与 `--help` 文本零变化(doc comment 原样搬移),契约测试不改自绿。
- **版本口径修订**:里程碑版本 = 0.<里程碑号>.0——M2 完成 = 0.2.0、M3 完成 = 0.3.0
  (协议冻结),1.0 保留给协议冻结后的正式稳定版、不再是里程碑承诺;取代 ROADMAP
  早期"M2=v0.3.0 / M3=v1.0"的写法,与 M1 期 0.1.x 的实际发版序列衔接;
- **token vs 截图对比指标搁置**(PRD §6,2026-09-27):截图方案 token 成本是业界公知,
  对比结论对用户无增量信息;actl 的 token 优势由 skeleton 削减率与输出体积承载,
  不再作 M2 门槛(待真实 agent 端到端场景需要时随场景量化)。

### 回归
- 金任务 1(计算器,batch 化)3/3;金任务 2(记事本全链含注入)10/10;验收 A/B/C 全绿。

## [0.1.2] - 2026-09-26

M2 收尾:分发三渠道 + crates.io 自动化发布 + 信号条交互打磨 + 时序评估闭环。

### Added
- README 示例化(安装三渠道/快速上手/信号条/安全设计/命令面);
- scoop manifest(scoop/actl.json,哈希与官方 checksums 交叉验证,autoupdate);
- Agent Skill(skills/actl:SKILL.md + references/error-codes.md 恢复手册,自包含无仓库链接);
- crates.io 发布准备:publish 元数据 + 版本化 path 依赖 + PUBLISHING.md 手册
  (actl 名被 2018 旧包占用,crate 名用 actl-cli、bin 名仍 actl;core dry-run 通过,
  实际发布需维护者 token 按序三条命令);
- **US1 十次重跑 10/10(100%,验收门 ≥90%)**;
- NFR 实测:冷启动 P50 54.9ms / P95 73.3ms(P95<80 达标;P50 微超 50——信号层两次
  状态写+锁探测的有意权衡,后续可优化为仅长命令写 begin 态);snapshot 全量 3.9KB /
  skeleton 2.4KB(削减 38%,记事本 23 个交互引用);
- 外部机器 US3 复现留待非开发机验证(README 安装三渠道已具备);
- crates.io 已发布:actl-core/actl-uia/actl-cli @ 0.1.1(cargo install actl-cli → bin actl,全链路验收通过);
- actl-signal 交互三件套:右键菜单(窗口/托盘共用:清除停止/显示隐藏/退出;修复托盘回调不区分鼠标消息——悬停即清停止的 bug);自绘品牌图标(运行时 32×32 DIB 绘制:深底圆角方+绿点,与状态灯同母题,单测验证 HICON 有效);HTCAPTION 无焦点拖动+位置持久化(signal-pos.txt,虚拟桌面钳制防拔屏丢失);
- **时序基建**:actl-core `Timing` 单源(9 参数:轮询/唤醒上界/探针/焦点链/分批/修饰键/锁等待),默认值=生产现值,环境变量覆盖(`ACTL_POLL_MS` 等)供 fixtures 免重编译扫描——未来"按应用自适应时序"的种子;`timing-profile.sh`(各应用延迟画像,仪器=envelope 自带 duration_ms)+ `timing-sweep.sh`(Q2 轮询检出延迟/Q3 分批吞吐/Q5 焦点链成功率);首次运行实测发现并记录:锁屏下注入被护栏正确拒绝(物理前台=锁屏界面)、UWP 僵尸框架后实例内容树不物化(计算器 buttons=3,需重装恢复);
- **时序扫描结论(docs/timing-2026-09.md)**:Q2 轮询 50/100/250ms 检出无差异(瓶颈是被等事件);Q3 分批 128 比 32 快 17% 但停止粒度×4,不值;Q5 焦点链全部 10/10 有裕量——**三默认值全部维持**;画像发现本日环境整体慢 ~3×(绝对值不可跨日比较,fixture 可随时重测);
- **信号条 UI 打磨**:主行 SEMIBOLD 字重(细字发虚反馈);目标分隔符 `→`→`·`(箭头字形落到中文字体链渲染粗笨);热键优雅降级修复(某次大改中丢失,被占时不再 panic,提示行自适应点击停止);
- **release×crates.io 同步发布**:推送 tag 同时驱动 GitHub Release 与 crates.io 三 crate(crates job:版本守卫 tag==Cargo.toml,按依赖序 publish;token 经 gh secret 配置)。

## [0.1.1] - 2026-09-26

M2 前两批:输入可靠性硬化 + 命令面 24 + fallback 定位链 + US1 跨 3 应用 +
显示信号层(actl-signal 状态条与紧急停止)。

### Added
- **M2 第一批·输入可靠性硬化**:
  1. **输入占用协调**(doc 09 §6.2):多 actl 实例互斥的会话级命名互斥体 `actl-input-lock`(RAII 释放,ABANDONED 接管,忙 → TIMEOUT 附恢复建议),press/type 注入动作段全程持锁;type 键盘通道**分批注入**(32 字符/批)批间复核物理前台,焦点被抢即停在批边界并如实上报(`delivered`/`stopped_early`);envelope 新增 `input_effects`(keyboard/clipboard/pointer,部分送达 = ok:true + keyboard:"partial")。单测 4 项:跨线程锁互斥、分批发完、边界停止、注入错误传播。
  2. **激活感知目标解析**:WM_NULL 探针(SendMessageTimeoutW,60ms,不阻塞调用方)测目标是否在泵消息;挂起 → ShowWindowAsync 唤醒 + 有界等待(≤1.6s),把挂起 UWP 应用每个跨进程 UIA 调用 ~2s 的隐形 stall 变成一次有界、可上报的等待;snapshot/find 上报 `wake_ms`。
  3. **`--physical` 规范裁定(spec)+ 物理鼠标链核心**:06 §2 规则 2 修订——键盘授权 = 强制 `--app` + 护栏 + 输入锁;指针物理注入必须显式 `--physical`(独占用户鼠标)。`click --physical`:UIA clickable point(无则拒绝,不猜中心点)→ 命中检查(WindowFromPoint 根窗口**标题**比对;WinUI 三件套使严格 HWND 比对不可行)→ 输入锁 → move+down+up 一批注入;虚拟桌面归一化(VIRTUALDESK,多屏/负坐标)。真机闭环验证:显示 7 → 物理点"清除" → 显示 0。
  4. **修饰键治理修订**(doc 09 §3.4):废弃"两端无条件清场 11 键"(会踩掉用户物理按住的修饰键);改为注入前检测修饰键物理状态,有按住则等待 ≤1.5s 后 NOT_ACTIONABLE 拒绝(按住 ctrl 时注入 's' 会被劫持成 ctrl+s;用户按键与历史残留不可区分,fail-closed);注入序列自拥有自释放(同批原子);`click --physical` 同样检查(ctrl+click 语义劫持)。部分 SendInput 失败的报错改为"状态未知,勿盲目重试"。
  5. **click 语义分发补齐**:Invoke → SelectionItem → Toggle(复选框/开关)→ ExpandCollapse(折叠面板/树节点),实际 pattern 随结果上报(此前无 pattern 的控件直接 NOT_ACTIONABLE)。
- **M2 第二批·fallback 定位链**:
  - **L1 精确 + AMBIGUOUS 结构化候选**:语义选择器多命中不再静默取首个——AMBIGUOUS 附 `evidence.candidates`(ref/role/name/automation_id,ref 与 snapshot 同源编号可直接重放,docs/12 §4 协议评审项落地;`CtlError` 增可选 `evidence` 字段进 envelope)。
  - **L2 名称模糊**:`name:` 精确落空后自动降级大小写/空白不敏感匹配(`fuzzy_contains`,`resolved_by:"fuzzy-name"`);`@eN` 不参与模糊化(ref 漂移 = STALE_REF 保持严格)。
  - **L3 序数直选**:新目标语法 `role:Type#N`(1 起,`resolved_by:"role-ordinal"`)。
  - **`--near <锚点>` 距离消歧**:多命中时取与锚点矩形中心距最近者(锚点接受完整选择器语法,须唯一匹配;`resolved_by:"anchor"`)。
  - `resolved_by` 真实上报("primary"|"fuzzy-name"|"role-ordinal"|"anchor"|"ref-replay"),click/get/set-value/type 支持 `--near`。
  - 真机验证(计算器):primary/AMBIGUOUS(34 候选)/role#2/anchor(--near id:clearButton)全通。
- **M2 第二批·命令面补全(24 命令 + 契约测试)**:
  - **指针族**:`right-click`/`double-click`/`hover`(clickable point + 命中检查 + 锁,命名即物理授权)、`drag --from/--to`(双定位 + 失败补发 LEFTUP)、`scroll --down/--up N`(UIA Scroll pattern 优先,pattern 宣称支持但调用失败时物理滚轮兜底——记事本实测);mouse 原语泛化(虚拟桌面归一化统管)。
  - **键盘持有**:`key-down`/`key-up` 半事件(必须成对;进程异常退出留下的按住状态由修饰键空置检查兜底拒绝)。
  - **窗口**:`resize-window --width/--height`(保位置 SetWindowPos)。
  - **verify 族(立即断言,与 wait 分工)**:`--exists`/`--value`(精确或 `--contains`,失败附 evidence.actual)/`--checked`(Toggle On);`click --expect`(RuntimeId 新窗口断言,与 press 同语义)。
  - **`wait --gone`**(等元素消失)。
  - **`extract --table`**:容器 → DataItem/ListItem 行 → Text/Edit 单元格的通用树表扁平化。
  - **envelope 契约测试**(actl-cli 集成,CI 安全):成功/PROTOCOL/usage 三态退出码、commands 自发现 schema 抽查 24 命令。
  - 真机抽样:verify(exact 失败/contains 通过)、right-click(菜单弹出→esc)、key-down/up、resize、hover、scroll(滚轮兜底)全通;命中检查 HWND 兜底修复(记事本窗口元素不暴露 NativeWindowHandle,按标题重解析)。
- **M2 第二批收官**:
  - **金任务 1/2 回归**:命令面大改(17→24)后双双 10/10。
  - **snapshot_id 版本化-lite**:快照记录落盘(LOCALAPPDATA,保留 20 份),`--snapshot <id>` 回放校验窗口 RuntimeId(同标题重开的新实例 → STALE_REF);元素级树前缀校验留待会话设计。
  - **US1 跨 3 应用首版 16/16**(fixtures/us1-cross-app.sh):计算器→记事本→资源管理器,磁盘+内容双验;资源管理器条目级确认因 UIA 虚拟化列为开放项(M2 第三批:ItemContainerPattern)。
- **显示信号协议 + actl-signal 伴随程序**(docs/11 显示层 Phase A/B、docs/13 拉取哲学):
  - **状态源四原语**:`%LOCALAPPDATA%ctl\state.json`(命令/目标/阶段/时间戳心跳,开始/结束/batch 步间落盘)、`stop-requested` 粘性停止标志(显式动作清除)、输入占用锁(已有,"正在操控"权威信号)、失联判定(>10s 无心跳如实显示失联,不显示成功)。核心为 actl-core `state` 模块(路径可注入,3 项单测)。
  - **执行侧闸门**:输入锁获取点优先检查停止标志 → 新错误码 **`ABORTED`**(spec:用户停止与业务错误、权限错误语义分离,清除标志后可重试);batch 步间双闸门(停止 + 步级状态)。
  - **`actl-signal`**(actl-uia 内第二 bin,保持唯一 unsafe 豁免点;纯 Win32 零新依赖):置顶半透明状态条(WS_EX_NOACTIVATE 不抢焦点,工作区右上角)+ 托盘锚点 + **Ctrl+Alt+F12 紧急停止热键**;250ms 全拉取轮询(锁+状态文件,零事件订阅——docs/13 事故价目表);五态渲染:空闲深灰/占用琥珀(命令+目标)/执行浅灰/停止红/失联暗;点击状态条=清除停止。
  - 真机验证(截图核对):空闲/占用两态渲染正确;停止标志 → 注入拒绝(ABORTED、零字符落盘)→ 清除恢复。热键待物理按键验证。
- MIT LICENSE(对齐 zotero_cli/article-mcp);workspace license/repository 元数据修正。

## [0.1.0] - 2026-09-26

M1 激进 MVP:纵向跑通"snapshot → 定位 → 操作 → 反馈"全链路;17 命令,
双金任务 10/10,冷启动 P95 53.9ms(实测)。首个可安装版本。

### Added

**命令面(17,按认知模型分组)**
- 感知:`snapshot`(UIA 树 + `@eN` ref,含 `--skeleton` 祖先链投影)、`find`(条件查元素,snapshot 同源 ref 编号)、`get`(属性白名单,含 ValuePattern)、`list-windows`(含 owned dialog)、`status`、`commands`(自发现 schema,clap 同源生成)。
- 执行:`click`(Invoke 优先 / SelectionItem 回退,实际 pattern 随结果上报)、`type`(元素置焦 + 键盘逐字,含 `--paste` 剪贴板通道)、`press`(组合键,`--expect window:<substr>` 后置断言)、`set-value`(UIValuePattern 直写,不经键盘)、`launch`(spawn + trailing args)。
- 窗口/流程:`focus-window`、`close-window`(WM_CLOSE 优雅关闭)、`wait --element/--window`(TIMEOUT 语义的耐心等待)、`batch --json`(单进程顺序执行,首错即停,禁嵌套)。
- 系统:`get-clipboard` / `set-clipboard`(CF_UNICODETEXT,ClipGuard 保证任何路径释放)。

**协议**
- envelope v1:统一输出契约、9 错误码(含 `ASSERTION_FAILED`/`INTERNAL`)、退出码三态 0/1/2(PROTOCOL→2)、可选 `snapshot_id`。
- 目标语法四形态:`@eN`(须 `--app`)、`name:<substr>`、`id:<AutomationId>`、`role:<Type>`。

**输入引擎(actl-uia::kbd,取代 uiautomation crate 的 Keyboard)**
- 双通道规则:组合键成员经 VkKeyScanW 映射走 VK 事件(修饰键语义成立,IME 不拦 ctrl+键);纯文本走 KEYEVENTF_UNICODE(绕 IME)。
- 通用修饰键 VK_CONTROL/VK_MENU/VK_SHIFT(AHK/pywinauto 同款),开场/结尾全场清场 11 键。
- 编译层纯函数 `compile_key_spec`(报错时零事件发出)+ 编译层单测 7 项与解析层回归(f1-f12 全段、双通道断言、shift 包裹、多键序列、不可映射组合拒绝)。

**护栏(fail-closed 族)**
- verify-then-inject:注入前物理前台(GetForegroundWindow)核对,焦点归属用物理层判据(GetGUIThreadInfo 上溯根窗口);press/type 强制 `--app`。

**验收与工程**
- 金任务 1(计算器 `123×456+789` → `56,877`):10/10,120 次点击零失败,`@eN` ref 重放 100% 稳定;实测 P50 40.2ms / P95 53.9ms / snapshot 275ms / 二进制 0.78MB。
- 金任务 2(记事本输入 → ctrl+s → 另存为 → 写值 → 保存):10/10,含内容读回断言与磁盘逐字终验;覆盖 owned dialog、系统对话框 ValuePattern、IME 对策。
- `cargo xtask setup/gate/fmt` 单点门禁(pre-push 与 CI 同源调用);Makefile 惯例转发层;git hooks(Conventional Commits 校验 + merge-conflict 检查);tag 驱动 release 流水线(verify→build→publish)。
- workspace 四 crate(actl-core / actl-uia / actl-physical / actl-cli);CI(windows-latest);文档体系 PRD / ROADMAP / TECH_STACK / 调研 01-11;AGENTS.md 协作规范。

**设计决策(入档,详见 docs/06)**
- 项目定名 **actl**(agent control);命令面设计(扁平动词式、命名四规则、L1-L3 分层与解耦/整合原则、近似命令对裁定);帮助系统三级层级与 stdout 协议例外条款。
- [spec] PRD 冷启动 P95 50ms → 80ms(附 P50 < 50ms):Windows 进程 spawn 固定开销 ~30ms 为不可压缩项。

### Fixed
- **[BC] `press`/`type` 强制 `--app`;[BC] `@eN` 未指定 `--app` 直接 PROTOCOL 拒绝**:无锚定的键盘注入与 ref 回放会打进"当下恰好前台"的窗口(事故 #1 的真实路径,曾实际偏射)。
- **组合键退化裸键**:`KEYEVENTF_UNICODE` 不携带修饰键语义,`ctrl+s` 曾被打成孤立 's' → 组合键成员改走 VK 通道;**纯文本被 IME 改写**(空格吞成上屏确认、半角标点全角化)→ 纯文本固定 UNICODE 通道,中文模式下唯一可靠文本通道为 `type --paste`;组合键成员不可映射 VK 时 PROTOCOL 报错,不静默降级。
- **owned dialog 不可见**:"另存为"等 IFileSaveDialog 挂靠宿主 depth-1,窗口枚举与 `--expect` 此前全部漏扫 → 两阶段扫描(root 快路径,0 命中才深扫;深扫进热路径曾致每命令 ~2s);`--expect` 升级为 **RuntimeId 基线的新窗口断言**(标题基线会被脏标记"*"改名骗过)。
- **窗口解析**:多匹配 → **AMBIGUOUS 附候选标题**(旧病:首个命中静默胜出);WinUI 三件套(ApplicationFrameWindow/TitleBarWindow/CoreWindow)折叠为单一可见窗口且保留 CoreWindow(内容元素只在其子树)。
- **焦点归属判据改物理层**:UIA focused element 在 WinUI 上会反向谎报(物理层已聚焦仍报别窗口编辑框),曾致成批误拒绝。
- **f10/f11/f12 解析边界**:旧条件 `p.len()==2` 把 f10-f12 挡在解析层而后端支持 1..=12。
- **窗口标题假信号**:效果断言改 UIA Value 内容读回 + 磁盘终验(新记事本设置页打开时标题仍是文档名、tab 名滞后只取首行)。

### Changed
- **[BC]** 全部 crate 由 `ctl-*` 更名为 `actl-*`,bin 名 `actl`。
