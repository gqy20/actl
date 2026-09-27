# 16 · 浏览器场景实测:内容域投影 + settle 谓词 + 小红书闭环

日期:2026-09-27 · 性质:场景实测轨(不进 CI;过程证据,结论沉淀于
[apps/browser.yml](../skills/actl/apps/browser.yml)) · 环境:Win11 zh-cn,
Edge(Chromium,系统自带,"个人" profile),开发机交互桌面

关联:[设计蓝图 §6](06-design-blueprint.md) · [ROADMAP 追加批](ROADMAP.md) ·
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

## 4. 三个实测发现(沉淀为 pack quirks)

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

## 5. 开放项

- 登录态 feed 闭环(标题文本可读性、open_item 点开笔记、site_search)——
  需用户扫码,未验路线在 browser.yml 保持 `unverified`;
- XHS 文档序数(#2)是否跨会话稳定——多会话复验后再决定是否固化进 pack;
- 平台风控节奏:注入是人类级信号,但连续机器速率循环有风险,采集类任务
  控制节奏(pack 已记)。

## 6. 结论

机制层(`--view content` 组合投影、`--stable` settle、text 通道)在真实
Chromium 页面全部按设计工作;数字上组合投影达标(71–80% vs PRD 70%);
场景层(读→滚→增量读→导航)闭环可用。小红书未登录态的内容限制是产品
形态(图片卡片),不是通道缺陷——登录态补测是下一步。
