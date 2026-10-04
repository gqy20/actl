# 81 · Word 样式排版流程：真机攻坚与验收记录

2026-10-04。任务链：直改字体对话框路线（flow3/5,多轮攻坚）→ 用户指出"没有真正选中"→ 行首选定区物理点击路线打通（flow6）→ 用户提议样式路线 → 探明/模板/协议/流程四步落地（本篇）。

## 1. 样式路线的设计与验收（最终形态）

**分工**：样式定义（字体/字号/行距）预置在模板资产 `skills/actl/workflows/assets/note-template.docx`（离线 patch styles.xml：标题1=黑体20pt、标题2=微软雅黑14pt、正文=宋体12pt，全部 1.5 倍行距），运行时**只应用不修改**；应用=光标式 `Ctrl+Alt+N`（作用于光标所在段落，**无需选区**）；编排=逐段「回车→剪贴板粘贴→按样式键」。

**真机探明（P1）**：
- Ctrl+Alt+N 光标式应用成立：基线 +中文正文/11 → 按后段落变加粗（标题1 特征）；
- 编舞落盘成立：三段探针保存关闭后 XML 为 P1→styleId 1、P2→2、P3→无。

**验收（note-typo-002，全 21 步 completed）**：打开模板副本→归属→逐段编排→保存→关闭重开→`verify-file --kind docx-styles` **逐段断言通过**（P1=样式1、P3/P8=样式2、其余正文）→优雅关闭。桌面成品 `actl-note-styled-20261004.docx`。

## 2. 直改字体路线的机制结论（对话框通道，供复用）

- 字体对话框（bosa_sdm_msword/「字体」）：`ctrl+shift+f` 焦点落字体框；**粘贴→down→enter(提交列表选中,对话框不关)→enter(OK)** 才真正应用——只按一次 enter 会以未变更的列表选中项确认=无效果（flow3 24 步机械全 ok 但零效果的根因）。字号框经 `ctrl+shift+p` 直接聚焦（初值=选区当前字号，含"小四=12"等中文名）。
- **读回通道**：重开对话框的组合框初值=选区当前字体/字号，`get focused:Edit --property value` 可读——对话框本身是格式断言通道。
- 对话框内组合框 UIA 名均为 "RichEdit Control"，name 定位失效；TAB 游走：字体→西文字体→字形→字号（×3）。
- 字号应用改用 `Ctrl+Shift+>`/. 逐级增长+每步读实际值（中文字号名解析表）比组合框提交稳。

## 3. 选区机制结论（本轮最大教训区）

| 原语 | 结论 | 证据 |
|---|---|---|
| ctrl+home+shift+end 等键盘行选 | **不可靠，从未真正建立** | 按键全 ok、焦点三视图干净，ctrl+c 后剪贴板为空（3 次重试）；对照 ctrl+a 全选+复制成功 |
| ctrl+a 全选 | 可靠 | 正文组 12 段全文核验通过 |
| 行首左侧选定区物理点击（用户教） | **可靠** | `click xy --physical` 后 ctrl+c 逐字核验，flow6 三组全部命中；坐标由截图+视觉判读提取，屏幕=图像+窗口原点 |
| 选区证明 | ctrl+c→剪贴板逐字比对 | 选错立即暴露；flow4b 标题组靠它拦截了空选区 |

## 4. 双层窗口与治理缺口（issue 素材）

1. **双层窗口**：外层 OpusApp + 内层 _WwG（标题飘忽"Microsoft Word 文档"）。内层持前台→type 前台守卫拒（不提示可重绑 hwnd）；框架层树坏（snapshot/find/front-report 全 INTERNAL "no usable runtime identity"）而 **_WwG 树干净**（Document→页面→Body, id:Body 可寻址）——值得做观察降级回退。
2. **对话框枚举盲区**：保存确认框/「对话框打开时无法执行此命令」错误框不在 list-windows（连前台窗口都列不出）；对打开的对话框注入命令会再生错误框（模态雪崩，实测堆到三层）。只能截图+视觉判读兜底。
3. **close 诚实性**：WM_CLOSE/SC_CLOSE 返回 ok 但模态未清时窗口不关；--pid 关闭因双层窗口 AMBIGUOUS。建议 close 后存活复核。
4. **popup_handlers 引擎治理真机生效**：`N:\ 目录名无效`启动错误框（本机每次开文档必弹）经声明（app=".docx" 锚 ⊆ 步骤 --app、error、name_contains、click id:2）由引擎自动处置，word-note-typography 全程无人工干预。

## 5. 其他沉淀

- **自动编号污染**：粘贴"N. 文本"到编号条目后新段→显示"N. N. 文本"；改用 ①②③④/• 规避（note-typo-001 验收发现后修正）。
- **shell 管道挂起**：actl launch 派生的 WINWORD 继承 stdout 管道，`$(...)`/Out-String 等不到 EOF 整链挂起——输出重定向到文件可规避；flow-run 引擎内执行天然免疫。
- **PS 5.1 双坑**：无 BOM UTF-8 脚本里中文字面量按 GBK 误读（中文名一律走 UTF-8 数据文件）；`cmd | tee` 吞 cmd 退出码（set -o pipefail）。
- **Word UIA 观察瞬态**：ctrl+s 后立即 verify 偶发 STALE_REF（note-typo-001 save 步三次未过、稍后手动同命令即通）——engine 的 retry_hint/恢复语义处理得当。
- 显示端 actl-signal 挂死一次（残影"录制未完成"+闪烁跟随鼠标；record-status active 为空=纯残影）：单次定点清理+display-start 恢复；显示端占用构建文件致 gate os error 5 的场景同日复现（AGENTS 预授权流程闭环）。

## 6. 交付物

- `skills/actl/workflows/word-note-typography.yml`（21 步,flow-validate 通过）
- `skills/actl/workflows/assets/note-template.docx`（样式预置模板）
- `verify-file --kind docx-styles`（协议新增,docs/06）
- apps/word.yml：quirks 4 条新增 + style_choreography 路线（verified）
- 验收产物：flow-resources/note-typo-002（冻结计划+证据）；桌面 actl-note-styled-20261004.docx


## 7. 提速轮（2026-10-04 晚，fill 编舞 + 模板 v2）

**结构**：模板升级为 **12 个预置样式空段**（P1=标题1、P3/P8=标题2、其余正文），运行时**零样式操作**；流程改为填充式——`type 文本 --paste`（剪贴板装载内化为单调用）+ `down` 跨段，3 组共 25 个输入调用（v1 为 ~40）+ 12 步（v1 为 21）。组级全文断言（精确前缀）替代逐段 contains。

**计时**：note-typo-004 全 12 步 completed（含引擎暂停恢复）；流程段合计 221s（含显示端瞬态无响应暂停一次）vs 002 轮 ~320s——约 30% 提速且含两次故障让行；干净单程估计 ~2.5 分钟。

**机制探明补遗**：
- 尾部 `\r`/`\r\n` 粘贴**不建段**（只产生软换行）；分隔符**内嵌**于文本中才建段（整篇一次粘贴 12 段成立，probe11）；
- 纯 `down` 计数在已填充文档中**漂移**（probe11：P4/P7 误中样式、P8 漏）——不做运行时定位地基；填充式设计天然 fail-safe：错位=文本错段=组级断言立即拦截，无静默样式污染。

**新发现（待立项）**：
1. **save 步验证缺口**：verify 只读活文档全文，ctrl+s 未实际落盘也"验证通过"（004 实证：文件保持模板原样 10407B），随后关闭被保存确认框卡住。修复方向：save 后断言文件 mtime/size 变化（元数据读取不受 Word 排他锁影响）——可作 verify-file 新 kind 或引擎写步内建。**✅ 已落地（同日提速轮）：`verify-file --kind meta`（size/mtime + size_min/mtime_min 关系断言），流程 save-disk 步接入。**
2. **保存确认框零坐标处置**：`click name:保存` AMBIGUOUS 时其证据携带候选 ref，按 `click @e5`（Button）直达——比截图判读坐标更快且无 DPI/阴影换算。
3. PS 5.1 引号缺陷第三次实证：`uia:{...}` 选择器经 PS 传参必坏（内层双引号被吃）；bash 单引号安全——uia 严格选择器只在 bash 通道使用。
4. 显示端第二次瞬态无响应（引擎 companion_unresponsive 自动暂停、自愈后 plain resume 续跑成功——治理机制按设计工作）。

## 8. 三层提速落地（同日晚，②③① 一次交付）

- **② locate 进程内元素缓存**：trace 实测定位热点 = `locate.dfs` 2.2-3.1s/次（每键 4.4s 的大头，Word 巨型树全树扫描）；缓存同 (app,target) 最近解析，命中以 **RuntimeId + 归属根窗口双复核**替代 DFS，失配回落原链路（fail-closed 不变），`resolved_by:"cache"` 如实标注；仅 name/id/role 静态选择器可缓存（@eN/uia:/focused:/--near 原链路）。
- **③ `verify-file --kind meta`**：`{size,mtime_unix_ms}` + 关系键 `size_min`/`mtime_min`（专用单调断言，剔除后空对象+subset=纯关系形态）；元数据读取不受 Office 排他锁影响——save 落盘断言的唯一打开期通道。单测覆盖。
- **① 流程砍重开循环**：save→save-disk(meta)→单次 close→离线 verify-styles 终态，10 步（原 12 步含两次冷启动+三次关闭周期）。
- **环境事故与教训**：诊断期对挂起命令的 TaskStop 在 UIA RPC 中途杀进程，留下 8 个不可杀的 actl.exe 僵尸（内核态卡死、锁死主构建产物，被迫启用 target2 备用构建目录）+ Word 反复楔死——端到端验收（note-typo-005）三次尝试均在 open 步受阻**挂起待环境恢复**（僵尸自行退出或重启后用 `target2/debug/actl.exe flow-run --file skills/actl/workflows/word-note-typography.yml --id note-typo-006` 补测）。**新纪律：actl 命令挂起时优先等其自身超时（一切等待均有界），不做 TaskStop 中途击杀。**
- 质量门禁：workspace 全测 0 失败 + fmt/clippy 干净（经 target2；正式 gate 待主产物解锁后补跑）。
