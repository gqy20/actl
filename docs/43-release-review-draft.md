# 43 · 首条业务流程:release-review-draft 交付与验证

日期：2026-09-28。流程文件 [skills/actl/workflows/release-review-draft.yml](../skills/actl/workflows/release-review-draft.yml)，
索引登记见 [流程索引](../skills/actl/workflows/README.md)。本记录只记已验证的事实；
写段联测未执行前不宣称业务闭环。

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

## 4. 写段联测(待执行,需用户在场)

前置:显示端就绪、Chrome 发布页为当前标签、本机无其他 Word 窗口。用户在 actl
卡片点一次"开始"后 flow-resume,预期一次跑完并核对:

- Word 新文档正文两行:"发布核对记录:" + 实测页面标题;
- 另存为页文件名框唯一定位(role:Edit 是否唯一须现场核对,否则收紧为组合选择器);
- 文件名框回车触发保存(若语义不符,改为点击 name:保存 按钮并处理潜在 AMBIGUOUS);
- Word 窗口标题变为 actl-release-draft - Word;默认文档目录出现同名文件;
- verify_draft 业务核验通过,流程 completed。

联测同时演练:写步骤中途强杀 → resume 走 verify-first / needs_review 路径
(已有引擎测试覆盖,真机路径此前未在业务流上走过)。联测结论回填本文件并同步
word pack(另存为路线)。此前不将本流程记为业务验收通过。

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
