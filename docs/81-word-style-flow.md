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
