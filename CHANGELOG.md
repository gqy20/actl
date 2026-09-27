# Changelog

本项目的全部显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0),
版本语义遵循 SemVer;0.x 期间破坏性变更在条目中以 **[BC]** 标注。
条目只记"变了什么与为什么";过程细节、实测战况与归因在 git 提交信息与 docs/(场景实测
记录、timing 报告)里,不在此重复。release notes 由本文件生成:正文中的
`@eN` 等 ref 语法必须置于反引号内——裸 `@` 会被 GitHub 解析为用户提及
(v0.1.3 曾因此把无关账号 en 列进 Contributors)。

## [Unreleased]

### Added
- **登录态深度闭环(docs/16 §5)**:用户 Chrome 完成两家登录后补测——① xhs
  "点开具体笔记 → 读正文+评论 → esc 返回"**全链成功**(ref 精点 @e164 →
  弹层 → 文本 4451→8170B,评论区"回复"×85);关键发现:**text 通道读整个
  文档范围而非视口**——评论区未滚到也能读,零滚动成本。诊断方法沉淀:
  UIA 文本与截图矛盾时以截图仲裁(未登录误判由此推翻——卡片标题在树里,
  是 find 默认视图没暴露全)。② bilibili 整页跳转形态:点击/新标题锚全通,
  评论读取未完成(卡点全在选卡/锚点层:整页跳转标题变锚失效、卡片标题
  AMBIGUOUS、活动卡两连踩、共享桌面锚漂移——五怪癖全部入 pack)。
  browser.yml:+5 quirks(modal_vs_page/card_name_ambiguous/
  text_reads_beyond_viewport/promo_card_trap/shared_desktop_drift),
  open_item 翻 verified,新增 read_note_comments(verified);fixtures:
  xhs-deep2/xhs-login-check/bili-deep5 定稿。
- **抖音深度实测(docs/16 §5.4):"读屏标签"开关发现 + 坐标缺口登记**:
  ① 抖音默认 a11y 半盲(文案/作者/推荐视觉可见不在树)——**钥匙是
  Button "开启读屏标签"**(抖音检测 UIA 客户端后暴露的开关,点开后内容层
  全进树,+294 行);② 互动层(卡片链接/评论图标)无 name 无 ref——评论栏
  展开需要坐标点击,**登记 M3 候选 `click --at x,y --physical`**(视觉
  兜底闭环的"手",与微信自绘右键菜单同源缺口);③ /explore 已 404,
  登录态首页为单视频沉浸流。三家横向 a11y 友好度:xhs > bilibili >
  douyin,pack 按站分级(2 quirks 入档)。
- **浏览器场景实测(docs/16)+ browser pack 翻档**:fixture 轨 8/8(四档投影:
  full 19764B → skeleton 43% → content 58% → **content+skeleton 80%**,超 PRD 70%
  线;settle 活体证据:10s 追加窗内阻塞 8410ms、观测 5 次变化);小红书真机
  闭环全通(地址栏导航 ctrl+l+paste → settle → 读 feed → 滚动×2 增量读;真实页
  组合投影 71%,较纯 skeleton 再省 38%)。三个发现沉淀为 pack quirks:多文档页
  AMBIGUOUS(xhs 2×RootWebArea,feed=role:Document#2,通用法 find 枚举+非空
  定位)、未登录卡片=图片对象(文本通道只见骨架,登录态留待补测)、导航须用
  paste 而非元素定位(临时窗口用完即关防 AMBIGUOUS)。browser.yml 5 路线翻
  `verified`(read/read_multidoc/tree/navigate/scroll),open_item/site_search/
  new_tab 保持 `unverified` 待登录态补测;新增 fixtures:edge-content-view.sh
  (CI 轨)+ xhs-browse.sh(场景轨)。**Chrome 对照实测**(xhs-browse-chrome.sh):
  全链同构通过,feed 同为 Document#2,组合投影 72%(Edge 71%)——pack 的
  "Chromium 系同源套用"断言就此实证,quirks 无一分叉。
- **`snapshot --view content` + `wait --stable`(spec,浏览器内容通道基建)**:
  ① **内容域投影**——只保留 Document 子树(网页/文档正文)与根→Document 路径,
  应用 chrome(工具栏/ribbon)整枝丢弃;与 `--skeleton` 正交组合,折叠仅作用于
  Document 内部、chrome 交互元素不参与保留判定——这是纯 skeleton 对浏览器形态
  仅削 ~36%(PRD §4 实测豁免项)的根因修复,目标是把该形态推向 70%+ 削减;
  窗口无 Document → `NOT_FOUND`(不静默回退);投影实现在 actl-core
  (`Projection` 枚举,keep-set 纯函数),`--view` 值域扩展预留 spec 通道。
  ② **settle 谓词**——`wait --element <t> --stable`:观测样本(text>value>name
  通道,避开 Chromium Document Value=URL 陷阱)连续两次采样(settle_ms=250ms,
  `ACTL_SETTLE_MS` 可调)不变即就绪,滚动/导航后的内容加载判定;超时 evidence
  带 samples/changes;与 `--property` 互斥("什么时候停"vs"到达什么值")。
  契约测试新增 view 值域/stable 互斥/schema 同源三项 golden;docs/06 §3.2/§3.4/§6
  同步。定位:浏览器/WebView2/Electron 是桌面 UI 主体,Chrome 136 起默认 profile
  禁 CDP,真实登录会话只有 UIA 一条路——本批是"真实会话浏览器自动化"赛道的
  机制地基(场景实测随 docs/16 跟进)。
- **截图→视觉工作流端到端实证 + pack 路线**:Notepad 实测 `screenshot --element
  role:Document` → PNG(24KB/2280×1180)→ 视觉模型**逐字读回**中英文混合内容
  ("actl + vision = the screenshot loop works. 敏捷的棕色狐狸。")——UIA 失明
  场景的完整读取链路打通。wechat pack 增 read_image_message(图片消息:定位
  bubble → screenshot --element → 上层多模态)和 capture_context_menu(右键
  point + screenshot --rect);SKILL.md 增视觉兜底协议节(三域用法 + 三典型
  场景 + "显式请求的兜底,默认路径永远是 UIA"定位);lint 76 步全绿。
- **PointerSpec 模型化 + 输入面三缺口收口(spec)**:① pointer.rs 的扁平
  PointerAction 枚举重构为 **PointerSpec**(button×clicks×modifiers 参数化,
  键盘侧 KeySpec 的镜像)——新增操作 = 新数据而非新分支;② **middle-click**
  (浏览器链接新标签/任务栏新实例)、**triple-click**(编辑器段落选,Notepad
  实测:三击 → ctrl+c → 剪贴板得到整行"Third and final line.")、
  **click --physical --with ctrl,shift**(修饰键与点击原子组合:按下修饰→
  N 击→释放,一批注入自拥有自释放;多选/范围选的刚需通道);③ 修饰键保持
  事件经 kbd::modifier_hold_events 复用编译层。契约 18/18。输入操作面至此
  规范化完整(操作×通道×授权×护栏×input_effects 全覆盖)。
- **裸修饰键 + 开始菜单可达(用户场景闭环)**:① `press win`/`press shift` 等
  裸修饰键此前被解析层拒("纯修饰键不是可按键")——放宽为修饰键本身即按键
  (与 key-down shift 同语义;单测+垃圾用例同步);② **开始菜单全可达**(更正
  普查的"待复测":窗口预创建故差集法失明;实测 snapshot --app 开始 = 搜索框
  Edit + AppsList + 钉住应用实名 Word/Edge/任务计划程序等);③ 用户工作流闭环:
  **自动隐藏任务栏 + Win 呼出**——`press win --app <前台>` 弹开始菜单并同时
  呼出任务栏,之后 `snapshot --app 开始` 全交互,esc 即收;系统 shell 交互面
  就此齐了(flyout/开始/桌面可达,仅任务栏按钮层退化)。
- **系统 UI 区域普查(docs/15)**:快速设置(Win+A)**全可达**(ControlCenterRegion,
  WLAN/蓝牙/勿扰按钮实名可点)、通知中心(Win+N)**全可达**(通知列表/请勿打扰/
  全部清除实名)、桌面图标全可达(ListItem)——系统控制原语路线(勿扰开关/WiFi
  切换)可直接进 pack;开始菜单待复测;任务栏/托盘维持退化终审。**更正**:Win 键
  一直被引擎支持(MODIFIERS 自始含 win→VK_LWIN,清场含双变体)——此前"安全边界/
  未实现"两说均为误判,如实入档;Win 组合注入的 --app 锚定语义 = 当前前台窗
  (任务栏不可聚焦)。
- **`snapshot --hwnd`(spec,加法)+ 特殊窗可达性修复**:Win11 任务栏 XAML 岛/
  TrayNotifyWnd 等无标题特殊窗此前双遮蔽(list-windows 滤空标题 + UIA root 枚举
  不含)。修复:`resolve_window` 的 Hwnd 分支改走 **ElementFromHandle 直取**
  (不再依赖 root 枚举),snapshot 增 `--hwnd`;实测 TrayNotifyWnd/XAML 岛可
  快照。**托盘交互现状结论(Win11 24H2+)**:任务栏 UIA 全面退化——根级仅
  Shell_TrayWnd、深扫 3 个空 Pane、XAML 岛树空,.NET 对照一致;托盘图标
  **UIA 路线不可行**,可行路线 = screenshot --rect 托盘区 + 视觉定位 + 坐标点击
  原语(待立项)或 Shell API 枚举(collector 式,非 UIA);溢出窗
  (TopLevelWindowForOverflowXamlIsland)待展开后复测。登记 M3 候选。
- **`screenshot` 命令(spec,新原语)**:像素捕获兜底通道——**显式请求**(与 --physical
  同级授权语义;默认观察仍是 UIA,定位哲学不变)。四域:`--app`/`--hwnd`(PrintWindow
  本体渲染,抗遮挡;bindings 缺失故手写 extern 声明)、`--rect`(BitBlt+CAPTUREBLT,
  屏幕所见)、`--element`(定位链 bounds 直裁)。**DPI 双 spike 结论落地**:捕获全程
  线程级 PMv2(collector 模式;实测 PMv2 线程内 UIA bounds 即物理像素——免比例映射,
  比参照实现更简更准);右键菜单类 UIA 不可见 overlay 不作窗口暴露(spike 实证),
  瞄准 = right-click 返回 point + --rect。PNG 经 WIC(纯 COM 零新依赖);卫生项:
  50MP 像素上限、shots 目录 keep-20、最小化窗口如实拒(不隐式恢复)、区域模式
  截屏幕所见(遮挡者入镜,help 明示)。envelope 只带 path/width/height/bytes——
  stdout 契约不破,视觉消费归上层。真机三域全通(区域 61ms、窗口 39KB、元素
  2280×1180)+ 视觉抽验清晰无错位;契约 17/17。
- 英文版 README(`README.en.md`),与中文版顶部语言互链;英文示例使用英文系统窗口名。
- **pack 契约 `skills/actl/PACKS.md`(pack_format: 1)**:字段与占位符语义、路线级
  `status` 信任分级(verified/manual/unverified,unverified 禁直接执行)、模板、
  贡献流程与 review 红线(盲发/规避护栏/核心语义冲突即拒)、私有 pack 外挂
  (`cargo xtask lint-packs <dir>`)与生命周期规则;excel/word/wechat 三 pack 升级
  format 1(补 pack_format/app_version/verified_date + 全部路线 status)。

### Changed
- README 规范化(面向用户重组):安装渠道折叠、快速上手收敛为"感知→执行→断言"
  三步、命令面改分组表格并修正过期计数(26→30,补上 0.1.4 的 `paste`)、安全设计
  改写为用户语言;开发者内容压缩为"参与开发"一段,内部进度黑话(金任务/US1/
  定位链四级)移出 README,详见 ROADMAP 与设计蓝图。
- AGENTS.md 过期的 License TBD 口径对齐为 MIT(与 LICENSE 文件一致)。
- **lint-packs 进 gate**(pre-push/CI 随之生效)并强化校验:pack_format 值、id
  全文件唯一(quirks/routes 共命名空间)、路线 status 必填且取值合法;支持目录
  参数校验仓库外私有 pack。
- README 命令面计数跟进 30→33(`screenshot`/`middle-click`/`triple-click`,
  前批 feat 的文档债)。

### Fixed
- SKILL.md 视觉兜底示例:`screenshot --element` 的作用域参数 `--app` → `--sapp`
  (二者 clap 互斥,原写法照抄即拒);wechat pack 游离于路线列表中段的顶层
  `verify:` 行并入 send_message 路线(原位置在 YAML 中非法)。

## [0.1.4] - 2026-09-27

M2 收官批:命令面 27/31 + 窗口治理与可观测性 + 信号条重设计 + Office/微信场景实测
+ 应用档案 pack 体系。

### Added
- **命令**:`wait --property` 谓词等待(value/name/checked,--contains 子串);
  `select`/`toggle`(SelectionItem/Toggle 直调);`paste`(ctrl+v 到当前选中——
  无元素落点的粘贴原语,刻意不做元素聚焦)。
- **窗口(FR3)**:`close-window`/`focus-window` 支持 `--pid`/`--hwnd`(多窗 AMBIGUOUS
  附候选);`list-windows` 每窗口补 `hwnd` 与叠放事实(foreground/topmost/minimized/
  visible/z_order/rect/owner_title)。
- **launch 通用修复**:裸程序名经 App Paths 注册表解析(`launch WINWORD` 不再
  NOT_FOUND);子进程 stdio 分离(GUI 程序继承管道句柄会挂死调用方)。
- **可观测性**:batch `steps[]` 每步结果与耗时(失败部分结果随 evidence 带出)、
  `windows_created` 窗口差集、步间 `blocked_by` 弹窗闸门(前台劫持检测,warning
  不阻断);`ACTL_TRACE=1` 引擎阶段打点(stderr,慢环境归因)。
- **skeleton 单链折叠**:非交互单保留子祖先折叠(ref 编号不受影响);token NFR
  修订为分形态口径(深单链 ≥70%;浏览器/小窗口结构受限,实测值入档)。
- **help 四要素**:全部命令的 -h 统一 Usage/参数/Example/Errors(表驱动 after_help,
  `build_cli()` 单点注入)。
- **actl-signal 重设计**:状态灯波纹动画(执行中)/呼吸(空闲)、条宽自适应、
  执行中实时耗时、Per-Monitor DPI(WM_DPICHANGED + 字体缓存失效)。
- **回归治理**:`cargo xtask regress` 一键全量(显式 Git Bash 探测,避开 WSL bash
  假败);全部 fixture trap 清理 + 前置断言;**GUI 应用全面弃用 taskkill**
  (会话复活根因);batch 窗口差集 + `close-window --pid/--hwnd` 构成清理闭环。
- **场景实测轨(docs/14,不进 CI)**:Office 三闭环双应用全通——Excel(GridPattern
  表格直读首次真实触发、剪贴板直灌+名称框导航读写、原地保存+磁盘终验)、Word
  (TextPattern 中英文正文、paste 写入);环境特性入档:受保护视图静默拒绝编辑、
  锁屏分层(UIA 后台可用/注入护栏拒绝)。
- **微信核心通道全通**:发送(读回门+三重验证)、会话搜索、文件发送(#32770 对话框
  Alt+O 确认——真"打开"是 SplitButton 无 ref)、联系人(列表/详情)、朋友圈(浏览/
  滚动/恢复)、@人(选择器 UIA 可见;U+FFFC 内嵌对象)、群信息;mmui 特性入档:
  Invoke 不触发导航须 --physical、右键菜单 UIA 不可见、rect-center 兜底(命中检查
  保留,实战拦截遮挡误点)。
- **应用档案 pack v1**(`skills/actl/apps/{excel,word,wechat}.yml`):路线知识固化
  为可校验数据,核心 CLI 不读(agent/skill 层消费);`cargo xtask lint-packs` 零依赖
  校验(步骤命令对活的 `actl commands` schema);SKILL.md 增弹窗协议、应用路线、
  actl×wechat-ui-collector 分工协议("实时操作用 actl,存量检索用 collector")。

### Fixed
- `ensure_element_focus` 前置免扰:焦点已在目标窗口时不再强行 SetFocus 首个
  Edit(Excel 多 Edit 场景旧启发式会吞掉后续按键)。

### Changed
- [spec] batch 输出与错误 evidence 结构(steps/windows_created/blocked_by)、
  `paste` 新命令、`wait --property` 新参数、list-windows 加法字段、help 注入
  机制——docs/06 同步;契约测试扩至 15 项。

### 搁置
- `list-notifications`:WinRT 依赖超出纯 Win32 边界,移 M3(蓝图 FR4 留位)。

### 回归
- 金任务 1 3/3、金任务 2 10/10、US1 16/16、验收 A/B/C 全绿;regress 跑后窗口与
  临时目录零残留。

## [0.1.3] - 2026-09-27

M2 第三批:观察层强化 + 定位性能结构优化 + 可观测性 + 信号条交互 + 版本口径修订。

### Added
- **L4 虚拟化容器定位(spec)**:定位链第四级——已物化树空手时走
  ItemContainerPattern 精确查询(Explorer 文件列表实证:树内 0 命中 → 命中未物化
  条目;US1 条目级确认收口);`find` 自动走 L4 并上报 resolved_by。
- P1 滚入视野(offscreen 元素物理动作前自动 ScrollIntoView);`get --property text`
  (TextPattern 正文专用通道,与 value 分工);`extract --table` GridPattern 直读
  优先 + source 上报;`list-windows` 物理叠放事实;`batch steps[]`;`ACTL_TRACE`。
- **定位性能结构优化**:`id:` 走 provider 侧属性条件(单次往返)、collect_matches
  惰性属性读(绝对收益待正常日复测)。
- actl-signal 交互:右键菜单/品牌图标/拖动+位置记忆/字重打磨/热键优雅降级;
  时序基建(Timing 九参数单源 + profile/sweep fixtures)。

### Changed
- **版本口径修订**:里程碑版本 = 0.<里程碑号>.0(M2=0.2.0、M3=0.3.0 协议冻结);
  token vs 截图对比指标搁置(不再作 M2 门槛)。
- CI:job name 引号修复(YAML plain scalar 含 ": " 致 0s 假败)。

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
