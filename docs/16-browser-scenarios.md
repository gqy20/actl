# 16 · 浏览器场景实测:内容域投影 + settle 谓词 + 小红书闭环

日期:2026-09-27 · 性质:场景实测轨(不进 CI;过程证据,结论沉淀于
[apps/browser.yml](../skills/actl/apps/browser.yml)) · 环境:Win11 zh-cn,
Edge(Chromium,系统自带,"个人" profile),开发机交互桌面

关联:[设计蓝图 §6](06-design-blueprint.md) · [ROADMAP 待验收工作](ROADMAP.md) ·
[PACKS 契约](../skills/actl/PACKS.md) · fixture:[edge-content-view.sh](../fixtures/edge-content-view.sh)
/ [xhs-browse.sh](../fixtures/xhs-browse.sh)

---

## 1. 赛道定位(一句话)

Chrome 136 起默认 profile 禁 CDP——**用户真实登录态的浏览器,非视觉方案里只有
UIA 一条路**。本批机制(`--view content` / `--stable`)就是这条赛道的地基;
全新上下文的 web 任务仍归 playwright/browser-use(分工见 SKILL.md)。

## 2. 机制验收(fixture 轨,可复现)

`fixtures/edge-content-view.sh`(本地页 + 慢速 JS 追加器,8/8 PASS):

**四档投影实测**(同页同刻,字节/元素数):

| 模式 | 字节 | 元素 | 削减 |
|---|---|---|---|
| full | 19764 | 403 | — |
| --skeleton | 11224 | 167 | 43% |
| --view content | 8369 | 160 | 58% |
| **--view content --skeleton** | **3878** | **58** | **80%** |

要点:此页"重内容轻 chrome",content 单独已胜 skeleton;**组合档是主通道**
(实测 80%,超 PRD §4 的 70% 线)。真实页(下节)content 单独反而大于
skeleton——内容交互密度高,组合才是解。

**settle 活体证据**:追加器 10s 窗内,`wait --element id:RootWebArea --stable`
阻塞 8410ms、观测 **changes=5** 后返回;返回后 text 通道读回追加器全部产物
(lazy-item-40 在场)。对照实验:内容静止时立即返回(samples=2)。

## 3. 小红书真机闭环(xiaohongshu.com,未登录态)

`fixtures/xhs-browse.sh` 全链走通:

1. **地址栏导航**:focus → `press ctrl+l` → `set-clipboard` + `paste` →
   `press enter` → 窗口标题变为"小红书…" → settle(**changes=1**,加载过程被
   观测到)→ 稳定;
2. **读 feed**:`get role:Document#2 --property text`(4454B envelope);
3. **滚动×2**:`press pgdn` → settle(各 changes=0/1)→ 增量读
   4454 → 4464 → 4625B(虚拟化窗口段净增长:旧卡卸载、新卡进场);
4. **树形削减**(真实页):full 42464B/888el → skeleton 19806B/299el(53%)
   → content 30942B/645el(27%)→ **content+skeleton 12366B/190el(71%)**;
   组合档比纯 skeleton **再省 38%**——XHS 内容交互密度高,skeleton 的
   削减大头(工具栏)不够,组合才是主通道的实证。

## 4. Chrome 对照实测(同案例,"同源套用"断言验证)

`fixtures/xhs-browse-chrome.sh`(用户真实 Chrome 会话,唯一标题起始页防
about:blank 歧义)——**全链通过,与 Edge 同构**:

| 项 | Edge | Chrome |
|---|---|---|
| 导航链(ctrl+l+paste+enter+settle) | ✓ | ✓ |
| 文档结构 | 2×RootWebArea,feed=Document#2 | **同**(枚举顺序略异,feed 同为 #2) |
| feed 读→滚×2 | 4454→4464→4625B(净增) | 4468→4478→4317B(第二轮净减) |
| 四档:full / skel / **both** | 42464B / 53% / **71%** | 45273B / 47% / **72%** |

滚动第二轮文本净减(4317 < 4478)同样是虚拟化的正常表现——旧卡卸载多于
新卡进场,窗口段内容换血;增减方向不作为判定依据,settle 的 changes 计数才是
加载信号。结论:**pack 的"Chrome 同源直接套用"成立**,quirks 无一需要分叉。

## 5. 深度层实测(登录态:点开具体内容 + 读评论,2026-09-27 第二批)

用户在 Chrome 完成两家登录后补测(`fixtures/xhs-deep2.sh` / `xhs-login-check.sh`
/ `bili-deep5.sh`;截图仲裁用 `screenshot --app`)。

### 5.1 小红书:深度闭环全通

诊断先行(三段法):UIA 读到的 feed 文本仍是"导航+页脚"骨架 → **截图仲裁推翻
"未登录"假设**(瀑布流完整、头像在场)→ 真相是**卡片标题在树里但 find --role
link 的默认视图没暴露全**(视口虚拟化物化)。用截图可见标题反查
(`find --name "Cherry Studio"`)命中 `@e164 Hyperlink`。

- **点击**:`click @e164`(ref-replay)→ 笔记**弹层**打开(左右分栏);
- **读内容**:Document#2 文本 4451B → 8170B,笔记标题、正文、话题、
  **评论区全在**("回复"×85、"共 X 条评论"×5);
- **关键发现:text 通道读到视口之外**——截图显示评论区在折叠线下方未滚到,
  文本里却已有 85 个"回复":Chromium TextPattern 读**整个文档范围**而非
  视口 → **评论不需要滚动就能读**(滚动 pgdn 后文本不变不是失败,是本来就
  读全了);
- **返回**:`press esc` → 文本回到 feed 形态(4479B)。弹层形态窗口标题不变,
  `--app` 锚全程稳定。

### 5.2 bilibili:点击/新锚全通,评论读取未完成(选卡层翻车实录)

B 站是**整页跳转**形态(同窗新标签),与 xhs 弹层互补,五轮实测抓到一串真怪癖:

1. **整页跳转 → 标题随页面变 → `--app` 锚点失效**:点击后窗口标题
   "哔哩哔哩"→"上海全球冠军赛…",后续命令全部 NOT_FOUND。**正解:跳转后
   重读 list-windows 换新标题锚**(或预记 hwnd 供 snapshot/close 用);
2. **卡片标题 AMBIGUOUS**:name 匹配 4 元素(Image alt + Hyperlink 共享
   标题)——find 后按 ref 精点即解(SKILL 纪律 4 的活例);
3. **活动卡陷阱**:推荐流混活动入口卡(VCT 电竞、AI 萌宠企划,两连踩,
   落地页无评论区)——**确定性选卡走搜索结果页**(keyword 用 ASCII 防
   GBK 污染),结果卡即真视频("古法做视频里加一点点科技 5.4万 01:56");
4. **视频页标题含站点名**:"…_哔哩哔哩_bilibili"——换锚逻辑不能
   `grep -v 站点名`,要排除的是**点击前的旧标题**;
5. **共享桌面锚漂移**:第五轮尾段窗口被用户接管(自动连播/人工切换,
   标题又变)——真机并发场景的活例,如实停止不再抢窗口。

B 站评论读取(真视频页)未完成,根因全部在**选卡与锚点层**,通道层
(click/settle/text)已被 xhs 闭环证明;复验走 `fixtures/bili-deep5.sh`。

**2026-10-11 复验轮增补:选卡与锚点层收口,新阻塞=shadow DOM(如实)**。

- **选卡层收口**:确定性选卡走搜索结果页(`browser-navigate` 直达
  `search.bilibili.com/all?keyword=<ascii>`)——`browser-read --all` 枚举
  42 张结果卡(title 全读),`--property href --all` 直接取卡片链接,**无需
  点击翻车面**(§5.2 第 3 条活动卡陷阱的更强解:枚举链接后 navigate 直达,
  绕开整页跳转锚漂移问题本身——§5.2 第 1/4 条锚点层痛点同此消失,会话
  绑定 tab 不随跳转失效);视频页标题经唯一读通道确认
  (「【JAVA解说】…TOP100!」)。
- **评论区新阻塞**:`#commentapp` 枚举只出一个 `<bili-comments>`
  **Web Component——评论在 shadow DOM 内,普通 CSS 选择器结构性穿不透**
  (滚动已注入 12 档确认非懒加载问题)。这是**协议缺口非路线缺陷**:需要
  shadow 穿透读取原语(browser-read 的 shadow-piercing 模式或受限 eval),
  登记 ROADMAP 新协议项,不在场景轮硬绕。
- `--all` 真机四假设全过(7 元素全枚举/--limit 3 诚实截断 total:7/空集
  ok:true+total:0/唯一模式同选择器仍 AMBIGUOUS 两形态共存+--property
  枚举 input 值);两轮会话均 browser-close 优雅关闭、ephemeral profile
  干净删除。登记的 `fixtures/bili-deep5.sh` 实际不存在(如实,场景知识
  已在本节)。

### 5.3 未登录 xhs(第一批遗留的澄清)

未登录态 feed **不物化**(树里只有导航+页脚+占位链接,截图确认无卡片)
——产品层登录墙,非通道缺陷。登录后卡片即物化(标题在 Hyperlink name)。

### 5.4 抖音:半敌对形态,"读屏标签"开关是钥匙,评论卡在坐标缺口

Chrome 登录态实测(三轮,迭代脚本不入库):

1. **`/explore` 已是 404**——抖音网页版卡片网格入口失效;登录态首页是
   **单视频沉浸流**(自动播放 + 右侧互动栏 + 左侧推荐列表);
2. **默认 a11y 半盲**:text 通道 21KB 几乎全是导航+页脚备案,视频文案/
   作者/评论视觉可见但不在树里;树里发现 `Button "开启读屏标签" @e112`
   (抖音检测到 UIA 客户端后暴露的读屏模式开关,默认关);
3. **开启后内容层全进树**(+294 行:视频标题、作者@、日期、时长、播放量、
   推荐列表全部文本)——`读屏标签`是抖音深度的钥匙,pack 已记;
4. **互动层仍是盲区**:推荐卡片链接、评论图标("785 条评论"视觉可见)、
   点赞按钮在树里无 name、不可交互(有 name 的 Text/Image 不分配 ref)——
   **评论栏展开需要坐标点击,actl 当前无坐标注入原语**。这是本场景挖出的
   真实机制缺口(与微信自绘右键菜单同源),登记 M3 候选:
   `click --at x,y --physical`(视觉兜底闭环的"手":screenshot+VLM 已能
   给出坐标,缺的只是点的原语)。

三家横向:a11y 友好度 **xhs(弹层+text 全读)> bilibili(整页跳转,锚点
纪律)> douyin(读屏标签+互动盲区,视觉兜底是终点)**——Chromium 系不是
铁板一块,pack 按站分级。

### 5.5 扩展操作面(2026-09-27 第三批,`fixtures/ext-deep3.sh`)

分析轮预告的三个未知点全部裁定:

| 面 | 结果 | 要点 |
|---|---|---|
| 工具栏按钮 | ✅ | name 在树里("猫抓\n可以访问此网站");**UIA Invoke 报 INTERNAL(E_FAIL)——必须 `--physical`**(页内元素 Invoke 全好,chrome 层按钮不行) |
| 弹层结构(U1) | ✅ | **独立 Document**(文档数 5→6,esc 后回 5);锚定即多文档枚举 |
| 弹层内容 | ✅ | **树形通道全读**:`--view content --skeleton` 的 name 采样拿到猫抓全部功能按钮(在线合并/下载/复制/反选/筛选/清空/深度搜索/缓存捕捉/更多功能/暂停/弹出);**text 通道空**(弹层文档不挂 TextPattern)——与抖音正相反的通道互补 |
| 键盘(U3) | ✅ | `press esc` 直达并关闭弹层 |
| 弹层存活 | ✅ | 观察类命令(find/snapshot)期间弹层存活 |
| 全标签扩展页 | ✅ | 普通网页 Document(既有结论覆盖) |
| chrome://extensions | ⚠️ | 视觉完全渲染(卡片+开关),**WebUI a11y 弱暴露**(text 空/toggle 无 ref;仅零星 Text 命中)——开关切换需坐标点击(同 M3 缺口)或键盘 Tab 导航 |
| 右键菜单扩展项 | ⏸ | 未完成:clickable point 两次被占(**用户桌面并发,护栏正确拒绝**——诊断三段法活例);机制存在待空闲复验 |

战略路线就此打通:**媒体嗅探**(猫抓:开弹层→读列表→"复制"按钮 name 就绪)、
**跨语言读取**(沉浸式翻译:触发翻译→text 通道读译文)可编排。Cookie-Editor
类凭据操作按安全边界跳过(白名单归上层 agent 策略)。

### 5.6 视觉兜底闭环端到端(2026-09-27 第四批,xy: 落地日)

`fixtures/douyin-vision-loop.sh` + VLM(本会话):抖音评论图标是坐标寻址的
第一个真实客户——

1. screenshot(3072×1920 物理像素)→ VLM 读出评论图标 PNG 坐标 (2568,983);
2. 首次换算用了 list-windows 的 rect 作窗口原点 → **窗口边界守卫拒绝**
   (坐标落在窗外)——守卫的活体证明:错误消息直接指引"re-screenshot";
3. 根因:list-windows rect 是**逻辑坐标**(非 PMv2 线程读取),200% 缩放副屏
   上与物理差一倍——**坐标系统一修复随同批落地**(rect 现 PMv2 物理口径);
4. 修正换算(物理窗口原点 + PNG 1:1)→ `click xy:6408,983 --app 抖音
   --physical` → **resolved_by:"coord"** → 评论区展开(截图仲裁:约 8-10 条
   可见,"全部评论"标签在场;"发送"按钮进树,评论文本仍不进——抖音评论区
   的文本层视觉兜底是终点,结论与 §5.4 一致)。

记事本 CI 轨 fixture(coord-click.sh)5/5:rect 中心点坐标点击 → 输入落焦
→ 文本 verify;窗外坐标 NOT_ACTIONABLE 拒绝。

### 5.7 寻址层打磨(2026-09-27 第五批)

分析轮列出的六项打磨落地情况:① wait 族 AMBIGUOUS 快速失败(fix,
fixture 1.3s vs 5s);② `id:#N` 序数;③ `--app hwnd:<n>` 锚;④ 非交互
元素物理点击**证伪**(机制在,但 Chromium 不暴露无语义文本——bare span
find 恒空,抖音卡片依赖读屏标签给的 name 才可能走此路,无名元素归 xy:);
⑤ ref 弹层漂移 hazard 登记;⑥ xy 闭环纪律入 SKILL。未动:settle 噪声
鲁棒、AMBIGUOUS 候选带文档上下文(登记 M3 候选)。

## 6. 三个实测发现(沉淀为 pack quirks)

1. **多文档页 AMBIGUOUS**:xiaohongshu.com 有 **2 个 `RootWebArea`**(主文档
   + 登录墙 iframe)——`id:RootWebArea` 一律 AMBIGUOUS;feed 正主 =
   `role:Document#2`(#1/#3/#4 是空壳,text envelope 113B 级)。通用法:
   `find --role document` 枚举 → 逐个量 text 长度找非空。settle 在多文档页
   同样须带序数(locate 失败被当作"未解析",samples 冻结 → 超时);
2. **未登录内容受限**:feed 卡片是图片对象(U+FFFC 对象符),文本通道只见
   导航骨架 + 页脚备案,无笔记标题——"图像盲区"的活例。登录态(扫码是用户
   一次性人工动作,正合 real-session 赛道定义)实测留待用户在场时补;
3. **导航路线的正确形态**:`ctrl+l`(聚焦+全选)→ 剪贴板 + `paste`(粘贴到
   当前选中,不经元素定位)→ enter。两个坑:`paste` 无 `--timeout` 参数;
   残留 about:blank 窗口会让后续 --app 匹配 AMBIGUOUS(临时窗口用完即关)。

## 7. 开放项

- 登录态 B 站真视频页评论读取复验(`fixtures/bili-deep5.sh` 定稿);xhs
  "展开更多评论"按钮未测(评论区 text 已直达,优先级低);
- XHS 文档序数(#2)是否跨会话稳定——多会话复验后再决定是否固化进 pack
  (Edge/Chrome 双浏览器已同构,跨会话待复验);
- 平台风控节奏:注入是人类级信号,但连续机器速率循环有风险,采集类任务
  控制节奏(pack 已记)。

## 8. 结论

机制层(`--view content` 组合投影、`--stable` settle、text 通道)在真实
Chromium 页面全部按设计工作;数字上组合投影达标(71–80% vs PRD 70%);
场景层(读→滚→增量读→导航)闭环可用。**深度层(登录态)亦已通**:xhs
"点开具体笔记 → 读正文与评论 → esc 返回"全链实测成功,且发现 text 通道
可读视口外评论(评论区零滚动成本)——"真实会话浏览器自动化"从导航采集
深入到内容消费层。B 站同深度差最后一步(评论读取),卡点在选卡/锚点层
而非通道层,怪癖已全部沉淀进 pack。
