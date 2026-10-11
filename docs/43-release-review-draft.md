# 43 · 首条业务流程:release-review-draft 交付与验证

日期：2026-09-28 · 写段联测 2026-10-11 通过。流程文件 [skills/actl/workflows/release-review-draft.yml](../skills/actl/workflows/release-review-draft.yml)，
索引登记见 [流程索引](../skills/actl/workflows/README.md)。本记录只记已验证的事实。

## 1. 业务目标与选型

读取已打开的 actl Releases 页(Chrome),在 Word 新文档生成核对草稿并以静态文件名
落盘。这是"从浏览器读取 → 生成文档草稿"模式的最小实例,同时覆盖 ROADMAP 综合任务
编排要求的观察契约、恢复核验、业务失效判据与写后核验。

选型约束(为什么不是别的):

- **音乐分享候选**:音乐窗口标题随曲目变化,v1 无插值且公共文件禁固定 HWND,
  静态计划无法锚定,待 M3 契约扩展;
- **夸克网盘上传**:上传携带磁盘路径(逐次输入)+ 进入网盘需截图定位物理点击
  (坐标禁入公共文件)+ 系统选择框绑定从未验证——三个不可静态化因素,
  已拆为 pack 层 `upload_file` 路线的独立待办;
- **记事本**:本机 `notepad` 文件关联损坏,连启动都不成立。

两应用(Chrome→Word)+ 全静态锚点(`Google Chrome`/`Word` 标题后缀稳定)是当前
协议下零逐次输入、一次授权跑完的最大可达自动化。

## 2. 流程结构(9 步)

| 步骤 | 演练点 |
|---|---|
| locate_page 读窗口标题 | outputs(JSON Pointer)+ max_age_ms=1800000 + stale_read |
| page_contract 页面内容契约 | verify contains,页面须是 actl releases 页 |
| open_word(检查点) | pause_before 授权点;resume_check 绑定标题做恢复后源稳定性核验;launch+wait --window |
| write_header / write_title | type --paste 写入;write_title 的 run/verify 双双绑定输出 |
| open_save_as → type_filename → confirm_save | Office 另存为页:role:Edit 输文件名,文件名框回车保存,wait --window 核对落盘 |
| verify_draft | 写后业务核验:Word 正文含实测页面标题 |

## 3. 已完成验证(2026-09-28,本机真机)

环境与证据目录 `target/workflow-release-draft-20260928/`(environment.json 记录
二进制哈希与 git 状态;validate-read.ps1 为执行脚本,逐调用 JSON/stderr/时间戳齐全)。

| 验证项 | 结果 |
|---|---|
| 协议预检 flow-validate | 通过(9 步;首次提交因绑定索引指向选项被拒,修正占位索引后通过——预检护栏有效) |
| 读段真机(r1) | locate_page、page_contract 完成,page_title="Releases · gqy20/actl - Google Chrome" 持久化;open_word 检查点暂停,attempts=0,全程未派发桌面动作(无 WINWORD 进程) |
| 时效拒绝路径(r2,本地 10s 时效副本) | 首跑在到达检查点前即因 output_freshness_failed 暂停(读段执行耗时已超 10s,引擎在消费步骤入口检查时效,先于任何动作);11s 后 resume 仍拒绝,open_word 保持 pending/attempts=0,零派发 |

r1 运行保持 paused 于检查点(30 分钟时效届满后该 run 无法继续,属既定语义,
重开新 run 即可);r2 为丢弃用本地副本,不入库。

## 4. 写段联测（2026-10-11 通过——release-review-10/11 两轮真机）

读段四步两轮全绿（v3 headless 轨）。写段经三轮结构修复后收口，最终形态 **release-review-11 全链 11 步零重试零阵亡 completed**，产物文件级核验通过（document.xml 含"发布核对记录："与 "v0.1.0" 两行）。

写段实测发现与修复链（全部入册 word pack quirks 与模板 v4-v5）：

1. **开始屏幕压过 /n**（09 轮实证）：本机 Word「启动时显示开始屏幕」优先于命令行 /n——裸 `WINWORD /n` 停在开始屏幕（深色主题下界面深灰，UIA 无 Document），写步必然 NOT_FOUND。修复：resources 复制 seed.docx，launch /n **带文件参数**结构性绕过；open_word verify 收紧为 `wait --element role:Document` 物化门（该门在 10 轮真实拦下一次 20s 超时——兼容模式冷启的瞬态观察失败窗口）。
2. **owned 对话框前台守卫死路**（10 轮实证）：F12 的另存为是独立标题的 32770 顶层窗，type/press 以步骤锚窗为 expected、对话框为 actual，永不匹配。修复：另存为链锚定**对话框自身标题**（`--app 另存为`），门禁补 `app:另存为` 的 popup_ignores 认领；文件名框用 uia 严格复合选择器（role:Edit 单独 23 个、name:文件名 跨 role 3 个，均 AMBIGUOUS）。
3. **另存为默认目录**跟随被打开文档（即任务资源目录 `${run.dir}`），非 Word 默认文档目录——模板注释按实修正；ctrl+s 对已带路径文档是原地保存，另存为改 F12。

产物：`${run.dir}/actl-release-draft.docx`（约 12KB，同名覆盖=当前核对状态）。10 轮另验证了 flow-revise 链（三次修订 lineage 留痕）与 re-verify 恢复路径（uncertain 步沉降后 plain resume 通过）；11 轮在前台快拒绝修复后零人工干预跑完（10 轮的两次 runner 看门狗击杀根因=Word 后台懒激活慢墙，收口记录见 CHANGELOG"前台快拒绝"条）。

原始预检清单（09 前预期，供对照）：两行正文写入/文件名框唯一定位/回车保存/标题变更/verify_draft 通过——全部按预期成立，唯文件名框选择器形态与保存目录两点与预判不同（已按实修正）。

## 5. 限制与已知风险

- `type --paste` 覆盖系统剪贴板(索引已注明);
- 草稿同名覆盖:actl-release-draft.docx 表示当前核对状态,留痕须自行改名;
- 多 Chrome 窗口(标题含 Google Chrome)或多 Word 窗口触发 AMBIGUOUS 拒绝,
  属 fail-closed 设计,先收窄再跑;
- 检查点后 30 分钟内未授权续跑则时效拒绝,须整跑重开;
- Chrome 当前标签切走后,恢复时 resume_check 的标题精确比对会失败并暂停
  (源稳定性核验按设计工作,不是误报)。

## 6. CDP 轨改造（2026-10-11，用户授权"改造"——写段联测的前置消除）

**改造决策链（三轮实证收敛）**：v1 前置"用户 Chrome 已开发布页"在 v3 消除——读段改 `browser-launch` 自有会话。v2 持久 profile+登录暂停点被两实证否决：actl 仓库**私有**（无登录态 404）+Edge 新 profile **首启向导弹窗链**（同步数据/翻译询问）污染现场；用户改判读源换公开仓库（gqy20/zotero_cli）。**v3 终态**：公开仓库+ephemeral+**headless**——零窗口零弹窗零登录，读源不依赖用户浏览器。

**读段联测通过（run release-review-07/08 前四步）**：launch(headless)→browser-list 产出会话句柄→`browser-read --css 'div[data-hpc] h2' --all --wait-for 2` 读最新 release 标题（elements[1]，探针实证：hpc 列表 CSR 异步渲染约 3s，索引 0=列表头 1=首个版本）→browser-close 会话治理。**两处模板修正入册**：GitHub 新版布局无 h1（标题=section 内 h1/h2 分层，latest 卡 h2）；open_word 的 launch 补 `/n`（无参数启动=开始屏幕，Document 不存在实证）；resume_check 反语义修正（缺席类前置不可用 verify 表达，删除）。

**写段联测受阻（慢 provider 日，如实）**：write_header 注入未落地+验证断言失败——Word MSAA 桥物化拒答回落+14.8s 慢扫（docs/81 已知同族）。写段在 v1 时代即未跑通（§4 悬置），非改造点；模板已全修正，正常速度日重跑。探针教训入册：读源探测应先一次会话收齐时序与选择器再改模板（用户指正"先探索再改"——本轮前四轮 run 即违反此序，逐轮失败）。
