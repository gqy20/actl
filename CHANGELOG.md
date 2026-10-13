# Changelog

本项目的全部显著变更记录于此，release notes 由本文件生成。格式遵循
[Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0)，版本语义遵循 SemVer；
0.x 期间破坏性变更在条目中以 **[BC]** 标注。条目只记"变了什么与为什么"；
过程细节与归因在 git 提交信息与 docs/ 里，不在此重复。

## [Unreleased]

### Fixed

- **[BC] 坐标系统一进程级 PMv2——`get rect` 在缩放屏曾回逻辑坐标(spec,2026-10-13 docs/83 §8 补遗,[06 §3.7 坐标系不变量](docs/06-design-blueprint.md))**:actl.exe 入口一次 `SetProcessDpiAwarenessContext(PMv2)`,整进程坐标兑现"PMv2 物理"宣称。实测根因:双 200% 缩放屏同一窗口,`get rect` 回逻辑 (1913,-7,1549×973) 而 list-windows/find-image 回物理 (3827,-13,3098×1946)——恰好 2×,两半管道单位不一致;agent 被迫人肉换算、物理点击被窗口边界守卫误拒(xy: 吃物理、guard 比对逻辑),AGENTS"缩放屏采错区域"教训(2026-10-12)同根因。修复后真机复验三通道对齐(get rect 3827,-13,3098×1946 ≡ list-windows;与 find-image 客户区仅差边框)。**[BC] 影响面**:缩放屏上 `get rect`/find 证据/preview bounds/focus-report/snapshot 几何的输出值从逻辑变物理(100% 屏无感知);下游以 rect 值做换算的编排脚本需复核。signal 显示端子进程本就进程级 PMv2(shell.rs/controls.rs 先例),不受影响;散点线程 guard 保留为无害冗余;设置失败仅 stderr 诊断不静默。

### Added

- **`set-value` 宿主容器信号 `host_control`(spec,docs/83 Phase 2,[06 §5.2](docs/06-design-blueprint.md))**:目标是 ComboBox 子 Edit 时 envelope 附 `host_control:"ComboBox"`——Win32 形态下直写子 Edit 不一定被宿主采纳(焦点离开才提交 ValuePattern,VMware 硬件页 CPU 核数实证:set-value 读回 4 而摘要栏不动,5 步排查后 type+tab 才生效);agent 据此在值未生效时先 `press tab` 或改 `select`,不再盲写。探测是直系 parent 两次 COM 往返,失败静默降级 None(信号是建议非判定,不阻断写结果);非 ComboBox 子 Edit 字段缺省(写入即生效)。help/docs §5.2 同步;vmware.yml quirk `combo_value_needs_tab_commit` 更新为信号指引。
- **`get --property selected` + `--verify-value` 读回三态(spec,2026-10-13 VMware 向导轮实证收口;[83 控件缺口评估](docs/83-ctrl-form-phase-survey.md) Phase 1,[06 §3.7/§4/§5.2](docs/06-design-blueprint.md))**:控件语义层两项补强,消除实战两类系统性误判。①**选定态读回**:RadioButton/CheckBox/MenuItem 不再只能截图确认——SelectionItem `IsSelected` 优先、ToggleState 回退(`On`⇒true),两 pattern 均不可用时为 None(非可选控件诚实降级);VMware 向导 8 项选择/勾选曾逐项截图核验。②**读回核验三态**:`--verify-value` 失败的 evidence.`readback` 字段区分 `mismatch`(读回可读但不等,真失败,保持旧消息与 readable:true)/`empty`(通道工作但恒空,reason=`readback_empty`,hint 指向 screenshot 核验勿盲重放——密码框/provider 背书 Edit 不暴露内容,**写入可能已生效**;VMware Configure Encryption 实证:掩码点 15 个已出现而旧语义报"读回不匹配",误导两轮排查)/通道不可读(NOT_ACTIONABLE `pattern_unsupported`,原语义不变)。golden `readback_empty_envelope.json` 钉死 empty 分型信封。测试:input_verify 单测 5(含 golden)+query.rs selected 序列化断言。

- **front-report owned 子窗文本信号增强(spec,2026-10-13,ROADMAP front 观察项候选①兑现)**:owned 原生弹窗表面新增 `texts[]/buttons[]`——`owned_child_texts`(actl-uia,EnumChildWindows+GetWindowText 纯 Win32 直采,不进 UIA provider,前台快门路径同样可用;文本 ≤16 行×240 字符、按钮 ≤12,去重截断)。动机:#32770 类弹窗全文在子窗 Static/Button,owned 只带标题是 decide 判读咨询置信贫困(conf 0.32-0.37<门槛)的根因;decide/front.rs 渲染行拼接 texts(≤4 行×240,判读 state 预算内)。**A/B 实测(真 Kev)**:同一 WMExternal Fail Fast 崩溃框信封,不带文本 conf 0.32 弃权 → 带子窗全文 conf **0.622 跨过 0.5 门槛**,family=crash 判读成立——「信号补全即恢复」预测证实;新式另存为(Explorer 壳,子窗是浏览器噪声)判读 unknown 属诚实行为,如实入册。真机链路:记事本 ctrl+s 真对话框 owned 层拿到完整 texts/buttons(另存为/编码/保存按钮族)。测试:单测 +1+契约 +1(texts 入 state/入 consult request);help front-report 四要素更新。无 golden 冲突(owned texts 可选字段,旧信封兼容)。


- **decide 应答质量三优化(spec,2026-10-13,jev-ultrafast/fastjev-egolite 对照吸收)**:①**领先裕度双门槛**——answer_card 消费 provider 完整概率分布,p1-p2 < MIN_LEAD_MARGIN(0.3) 即 abstain `low_margin` 附 top_choice/confidence;conf 只答「头名多可信」不答「比次名可信多少」,探针 t_g 实证薄裕度形态(0.604/0.228)恰是过 conf 门槛但次名有实质权重的答案;none 不参与裕度,provider 未返回分布时裕度让位(旧后端兼容,conf 独立工作)。②**AMBIGUOUS 截断护栏**——`ambiguous_error` 候选超 8 截断时证据带 `total/returned/truncated:true`(actl-uia),decide 见标志 abstain `candidates_truncated` 免网交回人——third-hand §13.2.1 召回保护(「排 536 位的正确目标不被裁掉」)的 decide 侧收口,在残缺选项集上问模型=把「正确候选恰好被裁掉」变成强迫硬选;旧格式信封缺标志按未截断。③**审计嵌卡**——decision-audit 每行附完整卡(≤4KiB 卡协议上限兜底体积),请求+应答配对成行=模型换版回归与阈值校准的**直接语料**(换 provider/模型版本后离线重放测漂移;golden 防不住首次错,语料防换版退化)——docs/08 §7 校准债的偿还通道。真机复验:calc-tie 真实分布裕度 0.80 正确放行(无误伤)/AMBIGUOUS conf 0.31 先拦门槛顺序无回归;单测 16+契约 21(薄裕度/宽裕度/无分布兼容/截断标志/截断免网弃权)。front 子窗文本增强(actl-uia 枚举,治 owned 层只有标题的信号贫困)按最小设计裁定为独立中等活登记 ROADMAP,不混入本 spec。

- **`flow-run --auto-decide` 探索决策自动应答(spec,2026-10-13 用户授权「这两个一起做了吧」)**:run 停 `decision_pending` 时 actl 自问配置的决策 provider(decision.json)逐次回填续跑直至终态——冷启动建 pack 的多卡任务从「出卡→人答→resume」三步舞变单命令。**授权红线与手工应答完全一致**:危险候选构造前剥离(provider 物理不可见)、任何弃权(dangerous_only/provider_none/low_confidence)停循环交回人(run 留 decision_pending,卡照常暴露)、回填走 `explore::execute` 同一校验链(成员校验/预算/急停)、停止/暂停逐应答边界让行、decisions 预算封顶循环(防御 cap=预算+1,撞上 INTERNAL 如实报);每答审计 `responder=auto_decide`(与 CLI decide 区分,校准数据分层)。预检先于 run 创建:静态流程 PROTOCOL(无决策卡语义)、与 --paused 互斥(暂停让行是人保留位);provider 配置缺失/不可达如实传播,绝不静默降级。实现 `flow/auto.rs`(file-lines 门禁:flow.rs 近拒绝线的拆分)+`decide::answer_pending_auto`(复用 answer_card 全管线)。**真机 A/B 双臂验收**:A=calc-tie 平局卡全自动 4.2s 完成(Kev c1/0.848 自动续跑 goal_met,actions=1/decisions=1);B=门槛调 0.99 强制弃权——0.848 被正确拦下,run 停 decision_pending/零自动执行,人工 resume c1 接手完成——「confident 自动续跑、弃权交回人」的授权语义真机钉死。契约 +2(静态流程 PROTOCOL/--paused 互斥/预检不留半注册 run/无卡可答时诚实传播不触 provider);help 四要素;PRD §1.3 授权条目/docs/06 §7.9/SKILL.md/flows.md 同步。任务模式闭环仍待数据立项,不在本授权内。
- **calculator pack popup_ignores 常驻表面认领(2026-10-13 门禁误报裁定收口)**:标题栏窗+主窗键面在 in_tree 是同名「计算器」的 unknown+带按钮常驻表面,前台态被 popup 门禁确定性拦写步(popup_pending:clear,双态实证后台快门放行/前台拦截);裁定走**既有声明制**(2026-10-11 降噪第二轮的 pack 来源机制,用户拍板),窄声明 `name_contains: 计算器` 只认领自体表面,真瞬态弹窗(标题不同)仍上抛。**裁定验证**:同形态此前确定性失败,声明后前台态 calculator-reset 流程 completed。calculator.yml 顶层声明+注释入册,WORD 轮同族路径(word-note-typography 重验)维持 ROADMAP 待办。

- **decide 真机收口轮三件全通(2026-10-13,桌面空闲轮)**:①**AMBIGUOUS 实机烟雾**——真实 3 候选信封(计算器「清除所有记忆/清除条目/清除」子串撞名)→ `decide --task '复位显示'` 推荐 c3(@e13 清除,CE)方向正确、conf 0.34 低于门槛如实弃权+resolved 回显 → 编排层采纳重放(ref-replay)→ 显示为 0 核验通过——第二消费者真实闭环;②**explore-decide-answer.yml 全链真机验收**——探索平局卡→workflow decide 步(run_id 经 `--set` 内联传入,同轮实证)→outputs 保存 choice/confidence→回填 goal_met completed;首轮验收抓到真 bug:**flow 步 outputs 的 pointer 相对 data 体不是信封**(`/data/choice`→`/choice`,离线 flow-validate 校验不出 pointer 层级错误,真机第三次证明验收轮不可省),已修并入 yml 注释;③**前台态 front-report 判读**——真实常驻表面(双窗口同名,31 键面按钮)→benign_surface 方向正确、conf 0.37 弃权+blocking 0.32 照报,与后台态捕获信封行为一致。popup 门禁误报双态证据齐(后台态原生快门放行/前台态确定性拦截)入 ROADMAP 裁定项。现场清理:计算器优雅关闭,残留差集清零。
- **decide 弹窗判读咨询形态(front-report 信封,2026-10-13)**:popup 门禁判读辅助的第五输入通道——`actl front-report --app X | actl decide --task '<流程意图>'`,owned/in_tree 表面渲染为有界 state 行后 **family 分类+blocking 概率并行一问**(System One 多问题原生形态,wire 从单 pick 问题泛化为问题映射)。家族集与门禁 handler 词表同义(crash/error_notice/save/confirm/update/notice)+benign_surface(常驻部件,计算器门禁误报轮的判读形态)+unknown(交回人)。**纯咨询**:family 是判读建议,处置仍归白名单与 unknown 上抛纪律;conf<门槛 abstain 而 top_choice/blocking 照报;front_clear 短路免网;幻觉家族 PROVIDER_UNAVAILABLE。无桌面实机烟雾(本地 Kev-4B):常驻表面→benign_surface、Fail Fast 崩溃框→crash 方向均正确,真实信封 conf 0.32-0.37 低于门槛——弃权+倾向照报,门槛在嘈杂真实载荷上拒绝自动断言(合成载荷同题 0.95:全文 vs 只带标题的信号差)。测试:单测 14+契约 20(front 形态 5 新例:短路免网/分类/blocking/幻觉拒绝/task 缺省)。docs/06 decide 节同步。

- **flow `--set key=value` 内联输入覆盖(2026-10-13,可重复)**:flow-run/flow-validate/flow-revise 三命令新增——换词重跑免值文件(`actl flow-run --file search.yml --id search-02 --set query=天气`);与 `--inputs` 值文件合并后 **set 优先**(同名键覆盖,重复只针对 --set 自身同键给两次);缺等号/空键 PROTOCOL,未声明键与值约束(16 KiB/NUL/`-` 前缀)沿用 inputs 统一裁决;覆盖值不落盘随 run 计划冻结。overlay 纯函数落 actl-flow/inputs.rs(与 resolve 同解析器消费)。契约 +1(换词/文件+set 并存优先序/重复键/无等号/未声明键,顺带钉"文件值无 --set 照常生效");实测 quark-search 换词换 app 全通。docs/06 可复用流程输入节同步。
- **decide 应答回路 YAML 化(2026-10-13)**:①`decide --run <id>` 第四输入通道——从 run 状态直读悬挂卡(`flows/<id>/explore.json` 的 `pending.card`),flow 步无 stdin 管道,这是应答步流程化的卡源;id 白名单 `[A-Za-z0-9_-]`(防路径拼接),与 --card/--task 互斥;②decide 入 flow `read_only` 分类(观察步,无写语义);③`skills/actl/workflows/explore-decide-answer.yml`——决策卡模型应答步(decide --run + choice/confidence 输出保存),回填 `flow-resume --decision` 属流程生命周期治理,nested-flow 守卫禁入步骤(与 batch/preview 同族,协议边界非流程限制),由编排层执行;④工作流 README 索引登记。契约 16 用例(--run 成功/未知 run/无悬挂卡/路径注入拒绝)。零模型重放不新增 fixture(同应用同 goal 已有 `replay-explore-s1.yml` 先例,本轮临时 batch 重放与之同构即废弃)。

- **`get --property enabled`（属性白名单补齐,2026-10-13 Broadcom 门户表单轮收口）**:UIA IsEnabled 进 `get` 白名单——禁用态此前只在写动作失败后可见,门户预填锁定字段(姓名/邮箱/国家类)的写前判读现为只读一步,配合 NOT_ACTIONABLE `target_disabled` 分型把"撞禁用字段"从多轮失败变为一轮预判。docs/06 §3.7 与 help 同步。
- **`actl decide` 决策卡模型应答命令(spec,[06 §7.9 decide 节](docs/06-design-blueprint.md)——Jev/Kev 类判别式决策模型接入,用户授权 2026-10-13)**:命令面第一个决策应答动词,也是**唯一出站网络触点**。读入三形态——裸决策卡 JSON / `actl flow-status --id <run> | actl decide` 管道 envelope / **AMBIGUOUS 错误信封**(第二消费者,docs/12 §4 结构化候选的消费闭环:`actl click name:浏览 --app MyApp | actl decide --task '<意图>'` 一线消歧,须配 --task——错误信封不带意图,缺省 PROTOCOL 附 task_missing 证据;候选按证据序映射,dangerous 词法闸独立重算,context 消歧行进判别描述,应答附 resolved 回显块 `{ref,role,name,automation_id,context}` 可直接重放不必回读错误信封)。**结构性剥离危险候选后**问显式配置的 System One 端点(`/v1/systemone`,Jev/Kev/laya/openjev 族;判别式 choice/confidence 类型化问答,非生成式 LLM——「不内置 LLM 客户端」PRD 边界不变),返回 `{choice, confidence}` 或 abstain(`dangerous_only|provider_none|low_confidence`;弃权非错误,run 保持 needs_review,裁决权不因模型移动);`none` 强制入选项,幻觉 choice(非 offering 集)拒绝。新错误码 `PROVIDER_UNAVAILABLE`(不可达/超时/负载不合规/choice_not_offered,原子命令不受影响)。配置 `%LOCALAPPDATA%\actl\decision.json`(endpoint/api_key/api_key_env/timeout_ms/min_confidence),`--endpoint` 显式且无配置文件时免配置直答(本地 llama-server/LM Studio 场景);审计 JSONL(卡 id/choice/conf/provider_ms/被剥危险数)落同目录,为阈值校准沉淀数据(docs/08 §7)。选型背书:两轮 18 载荷真机探针(本地 Kev-4B,target/kev-probe)——16/18 严格正确,两个失败形态(conf 0.37 自信错误/K>8 精度塌陷)恰好被 conf 门槛默认 0.5 与卡协议 Top-K≤8 结构性覆盖。**真机端到端三轮实证**:①真探索卡(calc-tie 平局 556B)→ Kev 答 c1/0.848 → flow-resume 回填 → goal_met completed;②explore-writeback 片段 batch 零模型重放全绿(飞轮「模型只教一次」闭合);③wire 契约 bug(questions 平铺被真 provider 400 拒)由真机验收抓到——离线桩忠实断言错误序列化,真机轮不可省。依赖:`ureq 2`(native-tls/schannel 同步阻塞,ADR-001 兼容)登记 TECH_STACK §3,actl-cli 直依赖 6→8(≤9 预算内)。测试:decide_contract 14 用例(std 桩 HTTP 全离线:危险剥离/none 强制/门槛弃权/幻觉拒绝/config 恢复提示/三种输入形态含 --task 误用与缺省/AMBIGUOUS resolved 回显/commands 自述)+ decide.rs 单测 9。
- **浏览器深水层基建（2026-10-13,用户授权「深度浏览器调用优先」后的选型落地第一步）**：`actl-browser` 引入 `chromiumoxide 0.9`（+tokio/futures,直接依赖 5→8,预算内）——全协议域类型化 CDP 骨干;async 圈在新增 `bridge` 同步门面之后(受控单 worker runtime + 通道投递 + handler 泵),crate 公共 API 保持同步,CLI/flow 调用方零改动。**ADR-001 相应窄修订**:桌面命令路径的对外 API 与派发链仍禁 async,例外清单加入「actl-browser bridge 同步门面之后」。选型证据:双候选三轮真机 spike + 三树全量许可盘点(`prototypes/browser-lib-probe/`,headless_chrome 因 GPL-3.0 协议生成器 auto_generate_cdp 出局)。浅层手写客户端(tungstenite/cdp.rs)原样保留——双栈过渡态,终态吸收由 ROADMAP 裁定。
- **`browser-netlog`(深水层首动词,spec)**:有界网络请求快照——`--window`(100..=60000ms,默认 5000)、`--filter` URL 子串、`--max-entries`(1..=2000,默认 500,触顶 truncated:true);仅绑 tab 的请求,enable 收尾必 disable,无响应体/头捕获(URL 清查面);read_only 分类入 flow 观察步。真机 smoke 实证:窗口内导航的请求被捕获(2 example.com GET,smoke 首轮暴露 target 发现竞态——attach 后立即 pages() 可能空表,修复=有界轮询至 3s;spike 未暴露因 new_page 显式等待)。cookie 姿态随深水层重述(docs/72 §8):类型面可达但**命令面**不可达,新增负面契约测试 `browser_command_surface_has_no_cookie_verbs` 钉死。TECH_STACK §3 登记三依赖(顺手修正 tungstenite 登记漂移 =0.29→=0.30),docs/06 命令族收录,docs/72 §13 死项清理(pipe 判死/batch 边界入册)。
- **flow 背景观察步 `background: true`(spec,用户裁定方案 a)**:只读观察命令与后续步骤**并发**的流程一等形态——引擎派发后不等待、流程退出前统一收割,结果入 StepRecord.background_result;语义边界(只读 run、禁 verify/outputs/pause_before 等控制组合)引擎 PROTOCOL 强制,收割失败如实改判 Failed(`background_failed:<step>`)。动机:B站场景轮实证 netlog 窗口×collect 并发是「观察 A 期间执行 B」的结构需求,shell 编排绕开 flow 治理是反模式(用户明确裁定)。契约:引擎 4 用例(派发时序/收割产物/失败改判/组合拒)+ input golden v1 增量;docs/06 YAML 接入段;AGENTS 验收脚本纪律第 7 条增补(裸 sleep 禁令扩展到编排层,并发观察场景一律 flow 背景步)。
- **`browser-targets` + `--target` 寻址覆盖(spec,扩展弹层写链收口)**:browser-targets 只读枚举会话浏览器的全部 CDP target(绑 tab 置首/其他 tab/扩展 popup 打开期间在场/service_worker 标注 dom:false);browser-read/type/click 新增 `--target <target_id>` 覆盖寻址——绑定重核与弹层门禁落在覆盖目标上,写护栏不因寻址放宽,覆盖时 envelope 回显 target。动机与路线:pack 实证扩展 popup 是独立 CDP target(2026-10-12),此前动词全部绑死 launch 时的绑 tab,弹层 DOM 结构性寻址不到;写链正道=物理开弹窗(UIA)→browser-targets 枚举→--target 读写其 DOM。契约:targets 未知会话 NOT_FOUND/畸形 id PROTOCOL 先于会话查(净 2 例+help 四要素);read_only 分类入 flow。

### Changed

- **[BC] `NOT_ACTIONABLE` 同码分型(spec,2026-10-13 Broadcom 门户表单轮实证收口;[06 §4 错误码表](docs/06-design-blueprint.md))**:一码多态曾共用一句 message+错误码缺省 hint,只读 message 的 agent 必然误诊。现按 evidence.reason 分型,各给独立恢复路径:`target_disabled`(message="target is disabled at action time",hint 指向 `get --property enabled` 判读——门户预填锁定字段不可写,盲重试永远无效);`target_identity_changed`(message 保持"target changed during action preparation",hint 指向重拍快照后重解析,勿盲重放);display 门控 `version_mismatch`(hint 指向 display-stop/start 同二进制配对重启+display-status 核验)。hint 渲染收敛单一源 `CtlError::effective_hint`(顶层信封与 batch 步级同错同形);golden `feedback_disabled_envelope.json` 钉死 disabled 信封。测试:feedback 单测分型断言+golden。
- **[BC] batch 步级契约补全(spec,[06 §3 命令组 batch 行](docs/06-design-blueprint.md))**:`steps[]` 此前只有 i/cmd/ok/duration_ms——读类命令的值在 batch 中全部丢失(表单 9 字段读取误判全空,直接引发连锁盲写),失败步 error 只剩 code/message(evidence/hint 在传播链丢失,"失败先读留证"协议在 batch 中失效)。现:步级回传该步 envelope `data`,snapshot 类步附 `snapshot_id`(batch 内产生的 ref 首次可被后续步绑定);失败步 error 与单调用信封同形(hint 恒在、evidence 有则保留),顶层错误 hint 继承首错精确恢复路径。命令字符串支持 shell 式双/单引号包裹含空格值("350 5th Avenue" 形态曾不可表达,与"3+ 顺序调用必须走 batch"纪律直接冲突);反斜杠保持字面(Windows 路径),未闭合引号 PROTOCOL 拒绝。测试:cli_contract 3 新用例(引号单 token/未闭合 PROTOCOL/步级同形)+query.rs 单测 3。
- **[BC] 虚拟化探测结构性拒绝归并 `NOT_FOUND`(诚实修复,[06 §3.7](docs/06-design-blueprint.md))**:`id:` 零命中回退 ItemContainer 查询时,provider 对查询本身的拒绝(`E_INVALIDARG`,Chromium 形态实证)此前冒充 INTERNAL 且 hint 指向无意义 retry——元素确已不在,诚实判定是缺席。现记录后继续扫其余容器(混排树可能有支持者),全空返回 NOT_FOUND 附 evidence.reason=`virtualized_probe_rejected`(channel/native_code),hint 指向"按缺席处理、可 snapshot 复核、勿原选择器重试"。
- **browser pack 两条 quirk 沉淀(Broadcom 轮正发现)**:网页表单 Edit 填写主通道= `set-value --verify-value`(ValuePattern 直写+同元素读回,5 字段实证 100%),剪贴板/type 只服务地址栏导航与 IME 场景;门户账户预填字段禁用态(get 可读、写入 `target_disabled`),写前 `get --property enabled` 判读,空字段才是写目标。
- **[BC] Phase 2 终态吸收:actl-browser 全动词上 chromiumoxide 骨干,手写传输层退役(用户指令提前于原触发条件)**——cdp.rs(单属主事件循环/421 行)与 `tungstenite` 直接依赖删除;launch 尾段(connect+new_page)/navigate/read/type/click/collect/verify/targets/close 全部经 bridge 骨干通道(open_lane 有界发现/eval_json 同 JSON 约定/订阅先于派发的事件等待);scripts.rs JS 与会话治理零语义变化,golden 契约零改动(30 用例原样全绿)。吸收期两真回归如实入册(均为骨干与浅层的**结构性语义差**,非平凡搬运):①http_get_json 移植时 `\r\n` 字面量变裸 LF——Edge 拒答、launch 健康检查恒败;②事件流订阅晚于导航派发恒 TIMEOUT(drain_events 是积压回放模型,EventStream 只收订阅后事件)——navigate 收束为 `navigate_and_wait`(订阅前置不变序),教训入 docs/72 §5。真机验收:9 步 B 站流程纯骨干全绿(33.8s,netlog 353 请求窗含评论 XHR 端点),现场零残留;体积 14.20→14.62MB(tungstenite 移除与 async-tungstenite 变体引入相抵,<16MB NFR 绿);直依赖 8→7。
- **[BC] 打包 skills 的 URL lint 收窄(用户裁定)**:旧 blanket「技能不含任何 URL」禁令过宽——浏览器自动化的**操作目标 URL**(站点/API 端点/ws 端点)是可操作事实,曾逼出 pack 里截肢式记录。收窄为真实风险面:**仓库身份指针**(自有仓库路径/用户名,裸写同拒)+`mailto:`;残余的「给人读」第三方外链归代码评审。顺手修复:workflows/README.md 泄露的 `Releases · gqy20/actl` 标题锚点参数化为 `<owner>/<repo>`(未开源前的发布包身份泄露,对第三方用户也是错误锚点)。

- **[BC] 单二进制 NFR 修订(PRD §4)**:深水层引入后主二进制 7.88MB→**14.20MB**(同机同配置实测),「<10MB」破线(用户裁定接受:深度浏览器能力优先);分发 NFR 口径改为「<16MB,零运行时依赖」——冷启动 NFR 不变。

## [0.2.3] - 2026-10-13

### Added

- **`set-value --verify-value`（与 type 读回核验对齐）**：Value pattern 直写后在原控件读回核验（同实例身份复核、2s 轮询、换行折叠精确比较，绝不重放写入），成功 envelope 带 `value_verified:true`。**已知边界如实入册（docs/06 §5.2）**：provider 背书的系统对话框 Edit（公共另存文件名框类）读回恒空——`--verify-value` 诚实失败而非假通过，那类目标走 `type --verify-value`（键盘路径读回可达）；空白读回不等于写入丢失。

- **`verify-file --discover <通配>`（产物发现断言）**：`--path` 为目录时取匹配锚定通配（仅 `*`、大小写不敏感、跨点号）的 mtime 最新文件断言，envelope 带 `discovered` 实名——保存对话框按类型过滤器补扩展名（输入 `.csl.json` 落盘 `.csl.json.bib`，Zotero 导出轮实证），预设精确名断言会静默落空；无匹配报 `ASSERTION_FAILED`（io_kind=no_match），与"文件可能稍后出现"同语义、参与 5s settle 窗（晚到产物被等到，契约测试覆盖）。

- **`desktop residue-report` 崩溃框残留谓词( 两起 WMExternal 崩溃框滞留后工具化——ROADMAP 条件触发项按「第二起同签名」兑现)**：只读全桌面扫描，应用无关的 WER 崩溃框族签名（Fail Fast Exception/应用程序错误/已停止工作/已停止响应/Windows Error Reporting）——**标题/pid 归属过滤对该形态必然失效**（「WMExternal.exe - Fail Fast Exception」不含主应用名、对话框 pid 与应用进程树无关，两起实证），故只认 WER 自身标题特征；命中带 hwnd/class/pid/可见性/前台+签名分类全文样本，零命中如实 total:0。定位：AGENTS「残留延迟复查」纪律的机器眼（时序错位残留——helper 在主窗关闭、收尾核对通过之后才崩溃——时点判定结构性覆盖不了，延迟复查+本谓词补齐）；收尾汇报纪律同步收紧：窗口清零≠资源清零，进程/托盘/崩溃框态一并报。处置纪律不变：错误框「确定」属白名单清理动词，工具只报不点。测试：签名纯函数单测（两起真机标题+本地化变体命中、正常标题不命中防噪音）+契约（清态诚实零样本+help 四要素）。

- **`app-locate` 软件位置五通道探测(spec,[06 §3 启动组](docs/06-design-blueprint.md)——腾讯会议误判教训的机制化收口:非标准盘符安装+App Paths 未注册,曾因探测漏开始菜单通道误判「本机未安装」)**:`app-locate <名> [--register]` 只读探测,通道优先序=①app-registry 别名→②App Paths 注册表→③常见安装目录(Program Files×2/%LOCALAPPDATA%\Programs)→④**开始菜单 .lnk 递归 ≤3 层含中文名**(IShellLink/COM 解析 TargetPath,MTA 公寓纪律同 capture——非标准盘符安装的唯一系统入口)→⑤PATH;去重后按序返回存在的候选+first,**卸载类快捷方式与可执行永不入列**(`Uninstall uTools.exe`/「卸载 腾讯会议.lnk」同样匹配应用名,first 指向卸载器=注册事故);有界(≤800 .lnk/每目录 ≤64);零候选如实 NOT_FOUND。`--register` 把 first 写入 apps.json(探测不启动应用);**manual 保护——用户显式指定(app-register)自动发现永不覆盖**:同名 manual 条目在场时 locate 照常返回候选但注记 `kept_manual`+现有路径,别名继续解析;register 层第二道防线拒绝 auto 覆盖;改用户指定必须走显式 app-register/app-remove;**手改 JSON 省略字段按 manual 解析**(人手写的条目就是显式意志,天然受保护);pattern 条目的运行时重解析只动 resolved 缓存,永不改 path/pattern 本体(用户指定的软件不会被运行时维护漂移)。**架构落点(pack 可移植性)**:机器路径只存 apps.json(机器本地),pack 的 `launch:` 字段统一收敛为**逻辑别名**(原 `{installed_executable}` 占位符四件 vscode/quark/zcode/tencent_meeting 全部换别名+注释)——其他用户拿到技能包后跑一次 `app-locate <名> --register` 即完成本地化。真机验证:腾讯会议(中文开始菜单→D 盘)/utools(program-dirs,卸载器已滤)/quark(App Paths→D 盘)/zcode/visual studio code(显示名查询)/垃圾名 NOT_FOUND 六形态+manual 保护双向(wemeet manual 条目 kept_manual 不覆盖/utools auto 条目可刷新)。AGENTS「先探测再写死」条目更新(探测动词=app-locate,别手工拼 reg/ls/find)。测试:契约(垃圾名 NOT_FOUND envelope+help 四要素)+手改 JSON 缺省 manual 解析/auto 覆盖防线单测。

- **探索统计剪枝 v2——UCB 前沿排序（spec,[42 §4.5/§11.4](docs/42-exploration-mode.md)——ROADMAP 探索模式条件触发项落地,机制先行+冷启动回落）**：`explore.stats` 新输入(选择器→{tried, effective} 计数,≤32 项)注入探索前沿——**样本足量(tried≥3)时 UCB 项替代该选择器的静态先验**(同知识不双计):exploit = eff_rate 映射 [−4,+2](0% 收敛 dead 先验,100% 已知有效)+ explore = c·√(lnN/n)(不确定者保探索余量,证据累积后衰减);**冷启动回落**——样本不足或零注入时行为与 v1 完全一致(纯向后兼容,稀疏样本的置信区间反而乱序,宁可回退已知语义)。数据闭环:writeback 新增 `suggested_stats` run 增量(每 tried 选择器 {tried:1, effective:0|1},有效=揭示新内容或位于 goal_met 最终路线),调用方跨 run 累积进 pack 后注入——分层铁律不变(核心不读 pack,计数显式入参)。**落地即生效形态**:当前样本量(5 条 verified 路线)撑不起统计验证,机制以冷启动回落形态在场,计数随应用轮自然积累后自动接管——真实统计下的真机对照轮(死候选自动压后/预算下降对比基线)待数据积累,留 ROADMAP 验证项。测试:统计层单测 6(冷启动回落/阈值边界/dead 收敛/有效抬升且忽略静态先验/半效率温和/frontier-熵门集成)+ schema 契约(effective>tried、tried=0、超界 PROTOCOL,有效形态通过)+ writeback golden(stats 增量入 golden)。

- **`browser-launch --user-agent` UA 覆盖（spec，[06 §3.4 浏览器组](docs/06-design-blueprint.md)——docs/73 v1 缺口「headless UA 壁垒」收口）**：headless 会话的合规通行道——HeadlessChrome UA 会被部分站点 403,`--user-agent` 覆盖 UA 字符串（可打印 ASCII,1-512 字节,入口前置校验——先于环境探测,畸形 UA 不依赖机器装有浏览器即可锁定契约面）。**如实边界**：只覆盖 UA 本身,不伪装指纹面（canvas/WebGL 仍如实应答 headless——站点做指纹一致性校验仍会识破）;持久 profile 复用活跃会话时不重申（进程已在跑）。launch payload 回显 `user_agent`。真机对照实证：headless 默认 `navigator.userAgent` 含 HeadlessChrome/155,覆盖后精确等于覆盖值。测试：契约（空/换行/非 ASCII/超长 PROTOCOL+合法值通过校验+help 宣告指纹边界）。

- **`notepad-save-handoff.yml` 保存框路径交接工作流（[workflows](skills/actl/workflows/notepad-save-handoff.yml)——ROADMAP 资源管理器保存框余项收口,[10 §4.2](docs/10-quicker-demand-research.md)「快选快切」需求的 actl 形态）**：把选定路径交给桌面应用的另存为对话框并磁盘核验——记事本裸启动→**点 `id:AddButton` 新建空标签**（TabState 复活容忍:历史脏标签留后台不清理,后台标签编辑区不在 UIA 树故锚恒唯一;用户既定做法）→录入内容→ctrl+shift+s→另存为对话框（锚对话框自身标题,`uia: role:Edit+name_contains:文件名` 复合定位绕开 id:1001 双命中）→set-value 清空→--paste 直填完整绝对路径→Enter 保存→verify-file 磁盘核验。**真机两轮**:handoff-01(用户显示端继续恢复)完成核心链、handoff-03(干净现场)8 步零重试 40s 文件逐字节一致。附带实证入 pack:explorer 地址栏实际形态 `Edit name:地址栏 id=TextBox`(XAML 双态,面包屑显示态 value 恒空——路径读取通道是窗口标题,调研 view_1012 与 26200 不符)、notepad 二启=**同进程新窗口**(非加标签,双窗即 AMBIGUOUS——流程前置零已存在窗口)、脏标签关闭确认是**应用内 XAML 对话框**(SecondaryButton 不保存=白名单清理动词,TabState 根清)、前台报错卡即时可见而 CLI 等整轮生命周期的显示端/命令端时钟解耦现场实证。

- **浏览器写步弹层门禁（spec，[06 §3.4 浏览器组](docs/06-design-blueprint.md)——browser-verify --no-popups 检测原语升级为写步自动护栏,与桌面 front-report「停下判读」同纪律）**：`browser-click`/`browser-type` 派发前自动扫页内可见弹层（默认选择器与 --no-popups 同源）,命中→**NOT_ACTIONABLE 拒发**附 total+样本+专用恢复 hint——模态态会静默吞点击/输入,派发成功 ≠ 语义生效,与其让动作空放后靠读回兜底,不如拒发让调用方先判读。`--allow-popups` 是判读样本后的唯一显式放行（流内静态 role=dialog 误报面）。dismiss 路径闭环：点弹层自身关闭键同样需要 --allow-popups,关后 browser-verify --no-popups 复核。门禁范围只圈「动作打到具体元素」的写步——navigate（CDP 层绕过 DOM）与 collect（DOM 级读不受覆盖层拦截）不设门禁。配套通用机制：`CtlError` 新增显式 hint 覆盖通道（`with_evidence_hint`,envelope 优先级 err.hint > handoff 特例 > 错误码缺省）——NOT_ACTIONABLE 缺省 hint（scroll/focus/set-value）对浏览器通道是错误恢复路径,错误契约必须自带真恢复路径。测试：契约（help 四要素宣告门禁+--allow-popups 解析面）+ headless 真机 5 项（click 拒/type 拒/verify 交叉同判/dismiss 放行/清层后直通）。

- **`find-image` 确定性模板匹配定位（spec，[06 §3.4](docs/06-design-blueprint.md) / PRD §1.3 边界修订——用户授权"截图视觉定位"非目标精确化）**：传统 RPA 图色定位的 actl 形态——ZNCC 像素相关在窗口表面定位参考 PNG,返回 PMv2 物理坐标(与 screenshot/xy: 同系)直接喂 `click xy: --physical`,补 UIA 失明目标(自绘菜单/frameless 标题栏按钮/canvas)的定位空档。**边界精确化**:PRD 非目标"截图视觉定位"指的是**语义视觉**(OCR/物体识别/VLM grounding,actl 刻意不竞争的赛道);确定性模板匹配是像素算术(与校验和同类),与"确定性、快、零模型成本"定位一致,故入范围。**零新依赖**:PNG 解码走 WIC(与截图编码同一套 COM,actl-uia unsafe 豁免);算法在 actl-core `template_match`(纯逻辑:金字塔粗搜 4× 降采样+积分图 O(1) 归一化+逐位相关→全分辨率精修→NMS,8 合成数组单测);原生胶合在 actl-uia(WIC 解码+PrintWindow 抗遮挡捕获+窗口原点换算屏幕坐标)。诚实语义:score<threshold 一律 NOT_FOUND(不降级猜测)、多命中无 --all 报 AMBIGUOUS 附候选、均匀模板(零方差 ZNCC 无定义)PROTOCOL 拒绝、模板大于窗口 PROTOCOL;`--scales` 搜 0.5×–2× 六档(DPI/缩放失配兜底,慢 ~6×)。模板像素精确纪律:目标机 `screenshot --element/--rect` 裁取,主题/DPI 变更需重采。flow 中为只读观察(read_only,可入 verify/when 位)。**性能(release 实测)**:3072×1920 大窗单尺度 427-548ms、--all 583ms、--scales 2206ms——最坏场景 ~2.2 秒,典型 <0.6 秒。测试:core 8+契约(help 四要素/阈值越界/--limit 从属/缺模板 INTERNAL)+真机 roundtrip 偏差 0px。

- **`browser-collect` 懒加载列表全量收集（spec，[06 §3.4 浏览器组](docs/06-design-blueprint.md)——浏览器轨对齐 UIA collect 语义；B 站评论/搜索结果流场景的原语补面）**：读全部匹配（复用 --all 枚举+可选 --deep shadow 穿透）→ 指纹差分去重累积 → 页滚一屏（CDP 写,声明动作卡 kind browser-collect,显示层协同同 navigate）→ 沉降（--interval-ms 默认 500）→ 再读,循环至**空连两轮零新增且已到页底**收敛（`converged_by:"empty-streak"`;触底信号不作完成判据——懒加载页 scrollHeight 会增长,UIA 侧位置信号否证的同款教训）。预算 --max-rounds（默认 100 上限 200）/--max-items（默认 2000 上限 5000）,触顶 `complete:false + truncation_reason`（max_items/max_rounds/scroll_failed 软截断）;读失败原样上抛;items 逐条 64KiB 截断同 read,`new_items_per_round` 逐轮留痕。测试：digest 稳定性/预算校验单测 + 契约（help 四要素/PROTOCOL 预算/NOT_FOUND/clap 互斥）+ **headless 真机全链**（懒加载 fixture 180 条滚动追加：180/180 全收 complete=true 首尾完整有序/max-items 诚实截断/读后核验/优雅关闭）+ **B 站真机应用轮**（搜索枚举→href 直达→deep 收集全链;抓出上述两协议缺陷;匿名评论墙判决入 pack quirk `bilibili_anonymous_comment_wall`——匿名只渲染 2 条评论+页面截断,全量需登录 profile,详见 [16 §5.2](docs/16-browser-scenarios.md)）。

- **`cargo xtask install`——本机安装链接布局（devtools 工具）**：把发布布局（exe×3+README/LICENSE+skills/）装进 `~/.local/share/actl/<label>/`,建立免管理员链接全套——`current` junction（升级只重指）+ `~/.local/bin` 三个 exe 硬链接 + `bin/skills` junction（pack 注入 exe 旁回退的落点）+ `~/.agents/skills/actl` junction（agent 知识面）。源支持 `--zip <发布 zip>` 或 `--release`（仓库 target/release+仓库文件本地组装）。安全约束：只替换经 reparse 校验的 junction 与既有硬链接文件,**拒绝删除任何真实目录**（如用户自建的 ~/.agents/skills/actl 会明确报错让人工处理）;版本目录就位后建链接,不留半链接状态。测试：roundtrip（安装/链接可读/版本切换 current 重指/真实目录拒绝）×2。手段说明：文件符号链接需管理员（实测拒绝）,junction 与同卷硬链接免特权。

### Fixed

- **存储不可写故障的错误码误用修复（维护区故障注入拆出的真 bug）**：存储目录不可写/磁盘耗尽时,`list-windows` 等命令曾报 `NOT_ACTIONABLE "execution state requires repair"`——把基础设施故障误导到 execution-recover 修复路径（修复动词治不了磁盘满/目录不可写）。修法：stop.rs 错误构造三分——**corrupt**（坏格式/版本不匹配,保持 repair 指引,原语义不破）/ **infra**（存储 IO 故障,INTERNAL 归基础设施故障码）/ **stop_record_json**（自产结构序列化失败,纯 INTERNAL）;全部调用位按语义归类（lock/读写/归档/recover_execution 的 IO 全 infra,内容校验全 corrupt）。测试：故障注入契约（LOCALAPPDATA 被同名文件占住时 status/list-windows/app-list/browser-list 必走 0 退或 INTERNAL envelope,禁 panic/裸退出;磁盘耗尽与不可写同注入族）+ 坏 generation repair 语义回归（execution_contract 5 项不破）。

- **lint-packs 去 release 重建——gate 的 58-74% 时间税（devtools/xtask）**：`cargo xtask gate` 的 lint-packs 步此前每次跑 `build_pair`（独立 target/validation 目录 release 全量编译 actl+actl-signal 双二进制）,而它做完全部 pack 校验只需要执行一次 `actl commands` 拿命令名清单——AGENTS 承诺的「pack 变更只跑 lint-packs（秒级）」实际是 3-5 分钟（三轮 gate 实测 lint-packs 占 216s/176s/311s）。修法：commands 查询改走**增量 debug 构建**（无变更 ~0.3s no-op;命令面与当前源码同步,gate 的测试步已把产物编好）;target/validation 的 release 对回归 regress/evidence 专用不变。实测：lint-packs 稳态 **1.6 秒**（原 216-311s,**24-34×**）,gate 整体相应 ~2 分钟化。附带探针轮定位：launch 慢失败（8.5-11.8s）=有界阶段叠加（Store 应用 spawn ~2s+UIA COM 初始化+忙显示端 500ms 查询×N+窗口轮询 1s+激活观测 1.2s）,无单点病灶,显示端探测退避已削其显示端泵竞争分量（入 [07 性能指标](docs/07-golden-tasks.md)）;MCP 每调用 spawn 的 ~1-2s 延迟特征作为有意安全取舍入档 [82 §5.1](docs/82-mcp-adapter-research.md)。

- **CLI 管道挂死族修三连（handoff-02 轮实证定位——「CLI 侧要等很久」的三层拆解）**：
  - **①launch 派生 GUI 子进程攥住管道写端（主凶）**：flow-run 引擎 4.4s 结束、envelope 已打印、actl 进程已退出,但调用方管道（`| head`/$/ReadToEnd/harness 后台任务）等不到 EOF——通用 `launch` 只做了 null 三件套,挡不住 Windows 句柄表继承：记事本/Word 等 GUI 子进程继承了管道写端,一直吊到 GUI 退出（实测 9 分钟,直到关掉 notepad）。浏览器通道/record-worker 早有同坑修法（`process_io::clear_stdio_inheritance`）,通用 launch 补齐该调用位。测试：**机制级活体回归**（CreatePipe 可继承写端挂为本进程 stdout→派生 5s 长命子进程→断言 EOF 不等子进程,删掉 clear 调用即 4.3s 失败）+ 真机 E2E（exec 直连 harness 管道,1.8s 返回而两个 notepad 实例仍在场——修复前该形态吊到 notepad 关闭）。
  - **②显示端 repair probe 固定 5s 轮询冷启**：run 暂停后显示端每 5 秒派生完整 actl.exe 跑阻塞步 before/verify 探测,放弃窗 10 分钟 = **120 次冷启**（机器高载时与真工作抢 UI 线程）。修法：指数退避（首个探测照旧即时派发,完成次数推进 5→10→20→40→60s 封顶;键切换重置）,10 分钟窗口降到 ~13 次;verdict 新鲜度窗随当前退避缩放,面板标签不闪断。AlreadyDone 语义核验为建议性标签（"可能是你手动完成了这步,请先核对"）无自动动作,维持。测试：退避序列/键重置/封顶 +4。
  - **③写步桌面交接 25s 阻塞等待收紧为 5s 宽限（spec,[32](docs/32-desktop-handoff.md)）**：显示端已出「开始/继续」提示卡后,CLI 进程内等用户点击的预算从 25s 收紧到 5s（`ACTL_HANDOFF_GRACE_MS` 可调）——用户在场两三秒即点,缺席时如实 NOT_ACTIONABLE 返回;flow 场景该步转 uncertain、run 暂停,显示端「继续」按钮是既有的恢复通道（handoff-01 实证）,CLI 不陪等人类。docs/32 同步。

- **临时 profile 删除竞争修复（docs/73 v1 缺口末条收口——五条至此全清）**：finish_close 对 ephemeral profile 的 `remove_dir_all` 单次吞错,浏览器进程树句柄释放晚于端点死亡时 `ephemeral_profile_removed:false` 泄漏目录（twitter 场景 7 轮中 2 次）。修法：有界退避重试（立即一次+200/400/800/1600ms,上界 ~3s;目录缺席=目标态已达成不进重试）,穷尽仍败如实 false。测试：锁文件独占句柄（share_mode 0）**活体复现竞争**并验证重试路径收口——单次实现（旧代码）此形态恒 false。

- **`--deep` shadow 文本提取两缺陷（B 站评论应用轮实证）**：① shadow host 元素的 `textContent` 只含 light DOM——嵌套 shadow（B 站评论 thread→comment 渲染器多层）的正文结构性读不到（恒空）；② 修复①后全文被 shadow 内 `<style>` 的 CSS 文本污染。修法：deepText 递归穿透嵌套 open shadowRoot + TreeWalker 过滤 STYLE/SCRIPT/TEMPLATE 文本节点——`browser-read --deep` 与 `browser-collect --deep` 同源受益。真机：匿名 B 站评论文本全通（用户名/内容/时间戳/回复数完整）。

- **browser-collect 提前收敛防御 + 滚动诊断**：纯空连两轮会在懒加载分段页提前收敛（页面未到底,更下方还有未加载分段）——收敛条件加"已到当前页底"（at_bottom 是继续滚的门而非完成判据;fixture 回归 180/180 不破）;payload 新增 `last_scroll`（scrollY/innerHeight/scrollHeight/at_bottom）诚实诊断。

- **pack 注入 exe 旁 skills 回退（[06 运行时治理④](docs/06-design-blueprint.md) 文档已声明/代码未做的收口）**：`inject_pack_popup_ignores` 此前只认 CWD 相对 `skills/actl/apps`——安装态（exe 链接进 bin,CWD 任意）pack 级 popup_ignores 注入会**静默失效**,而 Word 模板降噪是 pack 单一来源,失效=导航窗格挡写步回归。现 CWD 优先、exe 同目录 skills 回退（发布 zip 的 exe 旁打包布局）,首个存在的候选生效避免重复注入。测试：候选序纯函数 +1。

- **app-registry manual 保护（spec 增补——自动发现覆盖用户手工注册的真机实证）**：用户显式 `app-register` 的条目带 `kept_manual` 注记,自动发现通道（含 app-locate --register）永不覆盖;手改 JSON 缺省 manual,双道防线防静默顶掉用户的别名决策。

- **xtask install 跨卷回退（build 工具,真机 E:→C: 实证）**：仓库与家目录不同盘时 rename 报 os error 17——复制+删源兜底,安装布局跨卷可用。

### Documentation and validation

- **主行设备字形实现——显示层 spec 小项全项收口(ROADMAP 条目删除)**:琥珀态主行(Desktop 分支)前缀设备指示——键盘 U+2328(export 探针实证 DirectWrite 回退链出字形)、鼠标用文字「鼠标」(U+1F5B1 实锤豆腐块弃 emoji),剪贴板/焦点维持详情行不占主行;聚合口径与 details 影响行一致(多调用并发不遮蔽)。与 device 字段数据面(details 影响分列,§15.4 路线 B)合璧,显示层 spec 小项两条均收;glyph 探针测试 `export_device_glyph_candidates` 与主行断言 `amber_main_line_carries_device_glyph` 入库。glyph 渲染证据 target/glyph-probe/。

- **快照 materialization 回落态标注 + 显示层 glyph 前置实证(长期观察项两项收口)**:①snapshot/find 信封新增 `materialization` 字段(subtree_cache/node_by_node_fallback/node_by_node_legacy)——Word a11y 缓存回归轮的需求现场:回落态下文本通道天生为空,此前调用方无法从信封自查(实测 text_len=0 困惑轮);live 验证 4000 段 Word 文档报 node_by_node_fallback。②主行 glyph 裁定前置实证:export 探针渲染候选码位目视判定——U+2328 ⌨ DirectWrite 回退链出字形干净(VS16 可选,与 CJK 同行无挤压),U+1F5B1 🖱 实锤豆腐块;**待裁定提案入 ROADMAP:键盘用 U+2328,鼠标弃 emoji 改文字或 paint.rs 几何绘制**;探针测试 `export_device_glyph_candidates` 入库(产物即证据,与 export_flash_previews 同款先例)。

- **前台交接 v9 真机矩阵收口（,用户协同三场景——ROADMAP 长期观察项删除）**:S1 嵌套租约:flow 两步写动作审计 foreground_transfer/return 各×2,每动作一租一还;**首次转交被 Windows 拒绝(0x80070005)→授权落账 3ms 后重试 accepted=true**——「可拒绝合法请求」边界活体发生且被正确处理(拒绝入档+授权重试,无误报通过)。S2 人工接管:三步键入中用户物理抢键鼠——type_c completed/type_d uncertain+NOT_ACTIONABLE(注入打断如实报不确定)/type_e pending(闸门挡下)/run paused reason=human_takeover_or_grant_expired;input_monitor 外部输入增量入档。S3 接管恢复:面板继续→consent 重签→uncertain 步恢复核查失败→**needs_review/result_unconfirmed 禁止盲目重放**(docs/39 语义活体);同轮首证 control_log 对称修复的 source=cli 记录真机在位。fixtures v9-nested-lease/v9-takeover 入库;docs/62 矩阵补全,ROADMAP 条目删除。

- **交互摩擦 ⑤⑦ 收口**:⑤`verify-closed --instance` 改可选,三分支诚实语义——pid 消失→**决定性通过**(复用 pid 属别的进程,与 instance 无关);窗口仍在→**决定性失败**(原实例必然还在);pid 活而窗不在→NOT_ACTIONABLE 求签(复用与无窗原实例不可区分,instance 取 verify-owner 或 close-window 信封)——临时「关了没」核验不再被前置记录挡死,不可区分象限诚实报错而非假装通过。⑦裸 `e<N>` 引用别名:与 `@e<N>` 同义同约束(parse_target 新形态)——PowerShell 把裸 `@eN` 当 splat 语法吞掉(agent 实测两次踩坑),`e<N>` 免引号等价;此前为未知目标报错,无回归面。docs/06 目标文法+verify-closed 契约、SKILL.md、help 同步;契约测试:离线死 pid 决定性通过+e<N> 解析七断言。

- **CLI 交互摩擦轮(debug 实证驱动的五项修复)**:①`get --property rect`——元素物理像素包围盒 {x,y,width,height}(PMv2),此前元素坐标无 CLI 通道,调用方被迫手写 GetWindowRect 并踩 DPI 虚拟化陷阱(实测误判「面板被遮挡」的根因链);`screenshot --element/--rect` 信封同步带 `rect` 原点(此前只有宽高)。②指针族(right/double/middle/triple-click/hover)补 `--snapshot`——ref 漂移时右键想绑快照曾被拒,与 click 参数对称。③`front-report --app` 旗标——docs/06 契约本就写 --app 形态,实现只收位置参数属契约违背 bug;位置参数保留兼容。④`close-window` 信封带 pid+instance(进程创建时刻,与 verify-owner/verify-closed 同表示)——close→verify-closed 配对不再依赖事先记录;explorer.exe 的 shell 承载面(Shell_TrayWnd/Progman/WorkerW)PERM_DENIED 结构拒绝,ROADMAP 实录「拒绝属安全巧合」升级为护栏。⑤find --id 快路径 ref 问题澄清为文档修复:命中不赋 @eN 是形态而非缺陷——元素命令直接吃 target 形态(`click id:<X>`),docs/06 与 help 已讲明动作路径。测试:rect 映射单测/shell 护栏单测/交互人体工学 help 契约七断言。

- **探索引擎候选①收口:tried 资格化过滤（,docs/77 #5 无增量应用前沿空根因修复）**:XAML 无增量应用(计算器)点击不产生 added 元素,旧 tried 一刀切把首点后的按钮全部逐出前沿——应答「七」后前沿空 no_candidates,按钮序列任务死路。新语义:前沿与应答队列预过滤改 `candidate_eligible`——dead/no_effect 永久出局,**reveals(已证有效,changed-only diff 归因)保持可重选**,同按钮重复点击经应答者编排恢复;MockHost 序列测试 clicks=[b1,b2,b1] 实证(旧语义 [b1,b2] 即断),第三次点击无显示变化诚实归因 no_effect 出局。docs/77 #5 观察条目补决议;ROADMAP 引擎候选清空(剩④跨进程 picker⑤UCB 对照,均待真实场景)。

- **极端规模重树 A/B 完成——前提修订 + Word a11y 缓存回归定性（,Word 并轮）**:4000 段文档(手构最小 OOXML)实测推翻「Word 非虚拟化大树」前提——树结构=chrome+「页面 N」(Custom,Word_Page_N)+「页面 N 内容」(Edit,**全页共享 id=Body**,143 个同 id 为引擎③歧义收紧的活体样本);树随页数缩放不随段数(4000 段全量快照仅 481 元素),段落从不作为元素出现,逐段寻址不存在;双模式身份一致性保持(skeleton⊆full),全量 9.4s/skeleton 7.6s 无看门狗击杀。**同轮定性疑 Office 更新回归**:ACTL_TRACE 抓到 Malformed Cacheresponse pTreeStructure 物化拒答→逐节点回落(143 页 walk 6.9s),且原生 Word 模板对照轮**同签名**+body_read Document text 双通道空——时间线闭合:body_read 2026-10-04 verified 于 16.0.20326,现 16.0.20430 失效,Excel 两态探针同款剧本。word.yml:回归 quirk 入册+body_read 降 manual+app_version 刷新;ROADMAP「极端规模重树」条目完成删除、「拒答边界」改写为回归条目(复验条件=Office 修复版本;候选工程=快照 envelope 标注回落态)。过程修正如实记录:N:\ 错误框系本机已知怪癖(n_drive_startup_dialog quirk 在案),非正斜杠路径所致——初判归因有误,以 pack 为准。

- **维护尾项双清 + 探索引擎候选②③落地**:①CLI `flow-resume` 补齐 control_log 对称（显示端 resume 侧 requested/dispatching/child_returned 在位,CLI 侧此前只记 flow_event——Request record 包住多 return 分支体,requested→accepted/failed 与 Pause 同形;契约测试三态断言+unknown id failed 路径）。②进程中断中段验收:真实 TerminateProcess 中段杀 runner 的 WAL 专测（`real_kill_midstep`——WAL running 态如实报 interrupted,resume 闭合诚实失败态非 completed）。③探索引擎 `texts.focused:`/`focused:<Role>` 键（docs/77 已知限制②）:键入 OS 键盘焦点元素——合成候选 dangerous 标注**永不自动执行**（焦点可被任意窗口持有,键入必须经应答者显式裁决授权）,不参与投影关键词与定位预检,可达性由 CLI type focused: 角色核验 fail-closed。④frontier automation_id 歧义收紧（docs/77 发现③）:同帧重复 id 按观察序 `#N` 序数唯一化（CLI `id:<Id>#N` 同机制）,唯一者保持裸形（tried/priors/stats 键跨帧稳定）,name 子串保持裸形交 CLI AMBIGUOUS 如实上报。ROADMAP 维护两条清空（剩磁盘耗尽注入）、引擎候选收窄为①④⑤;docs/06 texts 文法与 docs/42 已知限制同步。

- **急停真按钮全链验收（,自 2026-09-28 悬置的核心信任闭环收口）**：用户真实操作走完「急停→解除」——右键菜单「停止操作」写入停止记录五字段全中（source=menu/pid/requested_ms/display_epoch/version 2）,停止代次推进;暂停中 run 保持 paused 不完成;`ACTL_EXECUTION_GENERATION` 绑旧代次输入调用 35ms 拒绝（ABORTED/execution_generation_changed）;详情窗「修复运行状态」解除后 ready/stop=null/新代次,新任务正常发起。语义澄清如实入册:急停拦「跨停止的旧代次调用」,新代次裸调用等效用户本人放行,急停后显式 flow-resume 可续跑纯观察步（写步由代次门禁拒）。真实失败通知（异常 paused,确定性 NOT_FOUND 演示 `notice-fail.yml`）目视两验,中文任务名渲染正常。两条 UX 观察入册:详情窗「继续/急停」视觉权重差（演练中误按继续完成旧任务,审计 signal_control resume 在案）与修复操作无可见反馈。docs 35/39/75 同步,ROADMAP 条目收窄为余项（接管暂停/严格时点/飞行中 ABORTED/OS 重启/DPI/减少动态）。AGENTS 诊断条款修正:结构性不可达目标（显示窗自排除）的独立只读观察是可达边界而非例外论证,独立截屏须 DPI-aware（非感知进程采错区域致误判「面板被遮挡」实证,后经用户截图纠正）。

- **探索模式接口缝隙收口（ 分析轮,P1/P2 落地）**：PACKS.md 新增「探索回写与隔离协议」——①双轨分工显式化:引擎轨 writeback 片段 vs 自由探索轨 agent 手工编辑 pack,两种知识形态不互相格式化,不建自动写入通道（verified 迁移=人工复核红线不变）；②隔离策略前置声明（quirk 形式,不新增结构字段）+ 真实实例探索协议五步成文（标记名产物/前后 find/白名单清理/find 空返回/residue-report 差集）;zotero.yml 以 `exploration_isolation` quirk 首个示范（single-instance 隔离不可行,实测 -datadir 转发给既有实例）。SKILL.md 增补:PowerShell 裸 `@eN` 被 splat 语法吞掉须加引号（实测两次踩坑）。AGENTS.md 验收脚本纪律增第 8 条:产物断言用发现不用预设名。

- **Zotero 导入导出真实库闭环（,用户本地实例授权轮——ROADMAP 应用深化条目关闭）**：剪贴板 RIS 导入→条目行出现在列表视图→条目右键「导出条目…」(菜单项在主窗 UIA 树内 MenuItem,非独立 HWND)→格式窗(Zotero 自有窗,id:format-menu 默认停在**上次使用的格式**——装 Better BibTeX 时默认 bib)→系统另存对话框(独立进程,文件名 Edit id:1001)→**产物落盘字段与导入一致**。三条通道事实入 pack:①系统另存对话框文件名 Edit 的 get value 读回恒空(通道失明),写入走 `type --paste --verify-value`;②写入后保存按钮 STALE_REF(对话框回流重排),文件名内容保留,重 snapshot 后点新 ref;③**Windows 按保存类型过滤器补扩展名**(输入 `.csl.json` 选 bib 类型落盘 `.csl.json.bib`)——产物后缀与格式类型必须成对核对,首轮"文件未出现"实为后缀误判。测试条目经 Delete→「移动到回收站」确认框清理,find 空返回核实;`export_csl_json` 路线保存段换用实证通道,`dynamic_refs` 写入验证范围扩至真实库。

- **腾讯会议 draft-date 键盘路线复验转 verified（,多模态会话——ROADMAP 应用深化子项关闭）**：视觉判读首页(自绘树 91 元素全无名 Group,UIA 无锚——多模态模型直接从截图定位「预定会议」入口 1421,977 物理点击,应用再无视觉轮依赖)→预定窗口 `schedule_meeting::ScheduleMeetingDialog` UIA 全在场(186 元素:主题/开始日期 QDateEdit/时间/时长/成员/会议室全可定位)→ctrl+a+paste+tab 键盘路线复验:开始日期 2026/10/12→**2026/10/15 精确替换+verify 读回确认**。`change_schedule_date` 路线 manual→**verified**;腾讯会议条目从 ROADMAP 应用深化整条移出。探测/安装/别名/bootstrap 交接四件此前已收,本条为该 pack 最后一块拼图。

- **浏览器扩展操作面探测补充（,persistent profile CDP 会话)**:①扩展在 **headless 会话同样存活**(CDP target 枚举见 chrome-extension:// service_worker);②chrome 层「扩展」按钮(id:view_1010)UIA 树可读、面板展开 305 元素全物化(扩展清单完整名称+权限项可定位);③**chrome 层菜单项 Invoke = INTERNAL 实证**(「未指定的错误」,pack 预判的 E_FAIL 风险坐实)——菜单项点击走 --physical;④**find-image 定位扩展浮动图标一发命中**(score 1.0/473ms,裁当前截图 60x60 模板)+物理点击触发扩展动作实证(沉浸式翻译图标=页面翻译切换)。扩展弹层内写动作成链的完整 demo 待多模态+物理坐标轮,操作面证据已入 browser.yml `extension_surfaces`。全部入册 [16 §5.5](docs/16-browser-scenarios.md) 同源 pack。

- **`--remote-debugging-pipe` spike 判死（,ROADMAP CDP 工程余量子项关闭）**：实测 Windows Edge 155 的 pipe 模式进程即退（exit 13）,同环境 `--remote-debugging-port=0` 对照存活——**pipe 是 Chromium 的 POSIX-only 实现,Windows 无端口消除通道**。风险如实降级:devtools 端口本就只绑 127.0.0.1 本地回环（launch 层硬编码）,非 LAN 暴露面;CDP 工程余量收敛为 uncertain 持久化+UIA 轨路线固化两项。

- **browser batch 组合验收（,ROADMAP 浏览器通道余量子项关闭)**:batch 内 browser-launch→browser-verify --newest-alive 组合通过(1.1s+45ms,sessionless 断言族天然组合);**结构性发现=batch 步间无 outputs 绑定,per-session 步骤(browser-read/close 需 --session)的 session id 无法步间传递**——该缝由 flow outputs+bind 层解决(browser-login-session 已验证),batch 层的组合边界=「sessionless 断言族可组合,per-session 操作走 flow 或两段 batch(中间由 agent 传 id)」。浏览器通道余量三子项(pipe 判死/uncertain 已覆盖/batch 边界入册)全部收口,子项删除。

- **Excel 两态探针（,疑似回归入档待复验）**：起始页 UIA 树健康（56 元素稳定物化,NavBarMenu/msotcid 族可导航,Backstage 新建页 92 元素）,原「UIA 树空属 Office 慢启动」登记**只在工作簿态成立**：新建空白工作簿后主窗 provider 观察连续 3×20s UIA 看门狗击杀+单点定位 12.7s 失败,等 45s 不恢复（进程健康 92MB）——非慢启动,是「新建空白簿」路径的结构性楔死。**与 pack verified 记录矛盾如实并存**:excel.yml table_extract/undo_redo（2026-09-28 verified,打开既有 XLSX 路线）当时 UIA 可用——疑 Office 更新回归或「新建空白簿」vs「打开既有簿」初始化路径差异;复验轮=既有合成 XLSX 打开路线重测,若打开路径健康则写入闭环解锁、新建场景以打开种子文件替代。附带三 quirks 入 pack:两态楔死探针/新建首日欢迎面板（UIA 读不到但自退）/close 保存确认延迟（2s 窗内 NOT_ACTIONABLE,窗外实际关净）。

- **Excel 写入闭环全链贯通（ 根因轮,上条探针的根治收口）**：①**根因判定**——「打开既有 XLSX」路径 UIA 全健康（180 元素稳定物化:名称框 1001/编辑栏 FormulaBar/DataGrid Grid 全在场）,矛盾解开:非 Office 全局回归,是「新建空白簿」独有的初始化路径差异;②**通道判定**——Excel 是 Word cache-only 边界的镜像（物化树 find 可见 DataItem id=坐标,单点 locate/set-value 通道失明、DataItem 无 ref 编号）;③**E2E 实证**——单元格写走 F5 定位对话框路线（press f5 → 对话框壳层 RichEdit **id:1027** 定位输入,引用位置 Edit 无 id 同 1001 双命中族 → enter 跳转 → ctrl+v 粘贴 → ctrl+s 保存）,B2 写入 **verify-file 权威确认**;Ctrl+G 被他软件全局快捷键抢占作废、名称框物化不稳定（帧间漂移 180↔178/无名态）、DataItem 无 ref 三条歧路如实入 quirk `f5_goto_cell_route`。cell_write 路线 manual→**verified**（F5 路线替换楔死的名称框路线）;写入纪律=一律 launch EXCEL <文件> 打开路线,新建场景用种子文件替代。ROADMAP「Excel 写入闭环」整条移出。

- **Zotero pack 探面 quirks 批量入册（真实实例授权探面轮）**：树暴露良好（711 元素全物化）/PDF 阅读页焦点陷阱（pdf.js 劫持 SetFocus,读回不符）/快速搜索双命中+set-value 路线/标签条菜单导航/新建条目与删除路线——两轮去重定稿。

- **文档体系一致性批**：全仓库文档废话清理与版本规范统一;残留延迟复查纪律入 AGENTS（时点核对覆盖不了关闭后 helper 崩溃）;docs/72 uncertain 跨进程持久化描述收正（引擎 WAL 写前落盘已覆盖,A1 内存 spike 过时描述删除）。

- **显示端重启后旧流程恢复子项收口（disp-restart-01）**：旧代次 paused run 经 display-stop→display-start（新 pid/epoch）后 flow-resume 正常走完 completed 零拒绝——中央任务提示验收子项。

## [0.2.2] - 2026-10-11

本版双主线：**稳定性终局**（尾段挂起族全链根治+僵尸清零、看门狗 P0 恒等式、前台快拒绝根杀后台懒激活慢墙、物化拒答回落——"慢日"重定性为自身污染并沉淀慢现场处置协议）与**编排/观察能力面**（for_each 有界循环、AMBIGUOUS pick 串联、popup 治理三层、collect 虚拟化收集、get-selection、浏览器 --all/--deep/--wait-for/--no-popups、focus 原语、MCP 适配服务、display-stop 生命周期动词）。docs/06 全面重写为纯现行态设计蓝图。

### Added

- **`display-stop` 显示端生命周期动词（spec，[06 §7.5 规则 8 例外](docs/06-design-blueprint.md)——"显示端占用构建文件"常设授权获得可执行实体）**：优雅关闭本工具显示端的**唯一 sanctioned 通道**——通用窗口命令对显示窗口的自排除（监督端不得成为被监督自动化的通用目标,2026-10-01 合成授予实证的防护基础）保持不变;display-stop 走 WM_CLOSE=普通关闭路径（撤销交接授权、发布 closed 租约、不写急停、不动停止代次）,只接受经**租约 pid 双核验**的自家显示窗（不构成通用关闭旁路）,幂等（不在场=already_stopped）,有界等待 ≤4s 超时如实报 stopped:false。显示端关闭期间写路径照常 fail-closed——"自动化关掉监督者"是自断双手而非提权,与 taskkill 无新增攻击面,差别只是走审计留痕的正规通道。MCP 排除表同步。测试：契约（help 四要素/reason 超长 PROTOCOL/缺席幂等;显示端在场时跳过行为断言）。配套 AGENTS"显示端占用构建文件"条款改指 display-stop。
- **popup 声明锚输入引用（spec 增补，[06 运行时治理③④](docs/06-design-blueprint.md)——flow 工具链缺口②收口）**：`popup_handlers[].app` 与 `popup_ignores[].app` 支持 `{input: name}`——模板锚为运行输入（如 `${run.id}` 前缀形态）时可静态声明 handler/ignore,不再靠 run id 前缀约定绕过（VS Code notice handler 探针实证的缺口）。在 load_plan 输入解析期展开,flow-run/validate/revise 三路同口径;引擎匹配语义不变;字面锚原样保留,未声明引用 PROTOCOL。测试 ×2。
- **flow-revise 收完整模板形态（spec 增补，[06 运行时治理④](docs/06-design-blueprint.md)——工具链缺口①收口,模板迭代不再全程重跑）**：--file 直接吃 workflow 模板——resources 块经 prepare 剥离（幂等,不建新 run）、{"input":...} 引用经 declared defaults 或新增 --inputs 值文件解析、pack 级 popup_ignores 注入与 load_plan 同口径。此前直接 schema parse,引用/资源形态必报 PROTOCOL——VS Code 轮被迫 emit-plan 具体化+带外 plan.json 手术（v2 报废事故）的根源。真机实证：vscode-edit.yml 模板原文件直接 revise 成功。配套：Word 新文档物化拒答边界入长期观察项。
- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门。**四个 Word 模板的手写声明全部撤除（pack 单一来源）**——新模板零声明成本即得降噪。测试：纯函数 2 + E2E 契约 1。
- **popup_ignores 弹层门禁降噪第二轮（spec，[06 运行时治理](docs/06-design-blueprint.md)——两个独立应用同根因升级为类问题后的声明式收口）**：门禁观察面分不清「常驻部件」与「真弹窗」——Word 停靠导航窗格（误拦）+ VS Code Find/Replace 面板（常驻 UIA 表面，关闭仅视觉折叠、带按钮挡一切写步=结构性死锁）两实例实证。**词表启发退役**（面板词白名单不泛化，第二实例即滑过）；**provider 可见性路线探针否证**（VS Code 折叠面板仍报 offscreen=false——Electron 对视觉折叠不说实话）。正解=声明认领：popup_ignores: [{app, name_contains}]，门禁三处同口径跳过；name_contains 必填（禁止掏空门禁的宽声明）；同表面 handler+ignore 双声明 PROTOCOL。测试：popup_gate +4。真机验收通过（VS Code 主轮同现场 A/B 对照铁证）。
- **popup_handlers `notice` 分类（spec，[06 运行时治理③](docs/06-design-blueprint.md)——INFO 级弹层无法声明的裁定落地，最窄形态）**：naive 词表覆盖不到流程自产的 INFO 级表面（VS Code「扩展已禁用」通知实证：滞留挡后续步、落 unknown 无法声明处置）。`notice` 认领 unknown 面板：`name_contains` **必填**（不存在「任意 unknown 一律处置」的宽形态）；已分类面板不被吞并；未声明的 unknown 仍永远上抛。审计面 popup_event 双字段（面板本体分类+认领声明分类）。测试 +3。真机验收通过（探针轮全链：匹配→自动点击→复查清层→写步 completed+verified）。
- **`focus` 焦点建立原语（spec，[06 §5.2](docs/06-design-blueprint.md)——「焦点断言/建立空档」基建项；VS Code Find 面板实证：Close 点击后焦点滞留，`press --target` 只断言不建立）**：UIA `SetFocus` 建立键盘焦点 + GUITHREADINFO 物理焦点根读回核验（3×120ms 退避），失配 NOT_ACTIONABLE 附 expected/actual。**零键注入**（input_effects.keyboard:"none"）——只建立焦点，后续注入由 press/type 既有护栏接管，注入语义严格分离。flow 中定位为写步。测试：契约 2。真机验收通过（focus 建立→press --target 断言式通过→verify focused:Edit 三重实证）。
- **`get-selection` 选区读取原语（spec，[06 §3.4](docs/06-design-blueprint.md)；Quicker 差距项落地——「对 AI 接口最有价值」的上下文通道）**：读当前用户选区，纯读取零注入。通道序：TextPattern 范围（kind=text）→ SelectionPattern 容器（kind=items）→ 如实 unavailable（不猜测不回退剪贴板注入——物理 Ctrl+C 通道须显式物理授权，留给调用方组合）。目标缺省 `focused:`。**边界诚实语义（真机实证入实现）**：UIA 折叠光标的 GetSelection 返回退化空范围——过滤后全空 = no_selection，不把"只有插入点"报成有选区。测试：契约 + 真机全链。
- **`collect` 虚拟化容器有界全量收集（spec，[06 §3.4](docs/06-design-blueprint.md)；形态裁定=引擎有界动词非 flow 循环）**：**先语义复位到顶**再滚动（每轮页级语义 ScrollPattern，**零输入注入**）× 沉降 × 每轮新鲜观察 × 差分去重累积。**完成判据 = 纯经验（空连两轮零新增）**；容器 RuntimeId 终检防重建误报。items 不带 `@eN`；预算 1..=200 轮/1..=5000 条，截断如实。**三处真机否证入档（Explorer 600 文件 fixture）**：ItemContainerPattern 枚举只返当前物化窗后假报耗尽——不采用；起始位置依赖——复位到顶消除；滚动位置信号提前一页误报——降为启发。真机：600 文件 **600/600 全收**（42 轮空连收敛）。测试：状态机单测 6 + golden + 契约。
- **AMBIGUOUS 目标串联（spec，[06 流程协议](docs/06-design-blueprint.md)——`flow-resume --pick @eN` 运行时候选纠正）**：run 相定位多命中时不再走通用失败语义——引擎锚定骨架快照（候选 ref 身份来源），候选证据全量保留，run 进 needs_review、步保持 Pending（零派发）。应答 `--pick @eN`：校验 ref ∈ 候选集后续跑时 args[0] 重写为选中 ref 并追加 `--snapshot`——身份核验交给既有 ReplayGuard；陈旧 pick PROTOCOL。flow-status 附 ambiguous_hint。pick 不改 plan_version,冻结计划四不变式全保留。**自动缩小查询裁定不做**（引擎侧静默改写选择器违背冻结计划显式变更纪律）。测试：引擎级 4 用例。
- **AMBIGUOUS 候选证据附 `context` 上下文行（spec，[06 §4](docs/06-design-blueprint.md)；pick 的前置零件）**：多命中候选证据在 {ref,role,name,automation_id} 之外，**重复标签**（含空名）附 `context` = 最近具名祖先文本——同标签兄弟控件从"调用方读整棵树自行分辨"变"证据自带区分度"。派生走索引父链上溯（walk 缓存读**零额外 COM 往返**，8 跳有界）。测试：纯函数 5 + golden 3。
- **`find --capable click|type` 动作兼容过滤（spec，[06 §3.4](docs/06-design-blueprint.md)）**：role 表派生的**建议性**候选池过滤——type = Edit/Document 文本载体，click = 可交互角色减文本载体；零额外属性读。把输入护栏从"执行时拒绝"前移到"选择阶段规避"。诚实边界：实际可操作性仍以动作时 pattern 分派为准；provider 快路径/全树/L4 命中同口径过滤。测试：单测 + 契约。
- **for_each 数据驱动迭代（spec，[06 流程协议](docs/06-design-blueprint.md)——「流程扩展：循环」落地；嵌套分支裁定缓办）**：`run: {for_each: {from: "stepid.output" [, max] [, field]}, body: [调用…]}`——列表驱动的**有界**循环：迭代对象必须是前置步骤 outputs 保存的 array，`@each` 子串占位注入项标量。有界纪律：次数=列表长度，max 兜底，超限 PROTOCOL **不静默截断**；项间停止/暂停检查；占位值守卫拒绝 `@eN`（每轮重新观察）。约束：禁 outputs/read_only-retry、verify 必填；body 禁嵌套、禁 bind。失败：第 i 项失败 → 位置证据随错误；已完成项重放幂等靠 body 内 ensure/幂等设计（诚实边界入册）。测试：纯函数 7 + 契约。
- **verify-owner 返回 `hwnd_anchor`（[06](docs/06-design-blueprint.md)；「flow 锚点标题插值」登记项降格收口）**：归属核验成功载荷附 `hwnd:<n>` 就绪字符串——outputs/bind 直填后续任意 `--app` 参数槽。**裁定入册：不做标题插值 spec**——HWND 不随标题变，"归属步产锚→后续步绑锚"用既有机制即可。真机（Explorer fixture）：目录改名使标题失配后，经 hwnd 锚 snapshot 仍寻址同一窗口。
- **`browser-read --all` 列表枚举形态（spec，[06 浏览器节](docs/06-design-blueprint.md)——ROADMAP 协议项落地）**：多匹配读动词——返回**全部**匹配：列表语义零命中 = 诚实空集 `total:0`（非 NOT_FOUND，与 find 枚举口径一致）；`--limit` 只裁返回面，`total` 保真 + `truncated_by_limit` 如实标记。**位置谓词绕行废止**。测试 +2。真机复验通过（证据二进制 fnv128=6c3d4e03）；首个真实场景消费（B 站搜索页枚举 42 结果卡+href 直达视频页——选卡/锚点层痛点整体绕开）。
- **`browser-read --deep` shadow DOM 穿透（spec，[06 浏览器节](docs/06-design-blueprint.md)）**：在 document 与每个**开放** shadowRoot 内递归执行选择器——Web Component 内部内容从结构性不可见变可读；closed shadow 是**如实边界**。遍历预算有界，触顶 `traversal_capped` 如实上报。真机实证（B 站评论区 shadow 内渲染器命中）。
- **`browser-read --wait-for N` 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数；与 --all/唯一读通用。背景:异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品曾是 output field missing 根因;外挂 ps1 轮询循环被用户两度指正——修基建让场景塌缩。真机:zotero_cli releases 页等 3s 沉降后返回 19 元素。
- **`browser-verify --no-popups` 页内弹层断言原语（spec，[06 浏览器节](docs/06-design-blueprint.md)——页内 DOM 弹层空档的 v1 检测面）**：原生弹层门禁结构性不覆盖页内 DOM 弹层——本谓词补检测面：枚举**可见**弹层表面（默认选择器 `dialog[open],[role=dialog],[role=alertdialog],[aria-modal=true]`，零布局盒不计；自绘浮层走 `--popup-selector`），0=通过、>0=ASSERTION_FAILED 带 total+样本。**flow 组合语义**：ASSERTION_FAILED→condition false 是既有映射——when/unless 弹层分支零新协议即可组合。测试：契约 2 + headless 真机六假设全落定。
- **探索引擎输入通道内容判据 + 锚点转移（[06 §7.9](docs/06-design-blueprint.md)）**：① `explore.texts` 的 type invocation 按内容自动选通道——**非 ASCII 注入 `--paste` 走剪贴板**（引擎观察不到 IME 键，只取内容判据），writeback 导出同口径——中文路线重放不退回键盘退路。真机（证据二进制 81ee14e8）：记事本中文探索轮 goal_met+三重物理证据，路线 #3 积累（apps/notepad.yml 首建）。② **锚点转移**：观察窗内动作打开的弹窗若为同进程跨窗，goal 探测把 `--app` 改写为候选窗重试；判据放宽为**同 pid 或 owner 归属链**——RuntimeBroker 跨进程对话框由 owner 链覆盖，同 pid #32770 形态有否定性数据入档（docs/77）。真机验收通过。
- **错误自述现场（基建五件套收口，[06](docs/06-design-blueprint.md)——诊断从"人回溯历史输出"变"错误自带现场"）**：失败时刻上下文汇流——错误 envelope 自述现场要素（前台窗/目标窗分类/窗口清单随 evidence 下发，failure_capture 采集包路径直达）；拒绝错误自述 execution 态（调用方直接看到"是谁持有现场"）。真机多轮（Word/VS Code/资源管理器）消费该面完成定位，evidence-first 诊断闭环。
- **MCP 适配服务 actl-mcp（spec 轮 1-2，[docs/82](82-mcp-adapter-research.md) / [06 §7.8](docs/06-design-blueprint.md)；PRD 2026-10-05 授权）**：远程访问被控 Windows 设备的独立服务——**独立 crate（不依赖任何 actl workspace crate，纯薄壳；async/tokio 仅限该 crate，ADR-001 定界修订）+ 官方 rmcp SDK（协议层不手写，用户裁定）**。① **传输模式 A**：`actl-mcp --listen [--token]`——默认 127.0.0.1，LAN 显式 0.0.0.0；token 缺省 256-bit + stderr 配对横幅（人工抄 token=设备主人知情授权）；Bearer 门禁 + Origin 校验；明文 HTTP 仅限可信局域网。② **子进程派发模型（安全穿越的结构性保证）**：每次 tools/call spawn 真实 actl 子进程（kill_on_drop）——预告/放行/急停/看门狗全部本地权威，**远端结构性无法绕过任何一环**；envelope 唯一权威，非法输出合成诚实错误不伪造 ok。③ **能力面三层懒加载**：通用 `actl` 工具（速查 ~700 tokens）+ `actl://help/{command}` 资源模板 + 全量 schema 不进资源面；排除表（操作本地监督机制的动词出局，排除不静默）。测试：库单测 9 + HTTP 契约 7 + rmcp 客户端互操作 1。真机跨设备验收待用户场景。
- **`cargo xtask doctor` 一键诊断 + 构建锁自动规避（[xtask](crates/xtask/src/main.rs)）**：进程普查（活/幽灵分组）+ 四产物锁探针 + ghost 残留映像盘点 + 显示端状态;`dev/evidence/gate` 构建前置自动 ghost-rename 被锁产物。**`doctor --clean`** 顺带删除 ghost 残件映像（判定扩展覆盖手工改名形态,仍被进程持有的如实保留）;配套纪律入册：脚本文件纪律只约束执行形态,进程普查/锁探针/残件清理这类管辖面直达工具。

### Changed

- **子树物化（[06](docs/06-design-blueprint.md)——观察面 N→1 往返收口）**：树观察缓存请求 `TreeScope=Subtree`——`build_updated_cache` **单次调用**物化整棵子树，遍历走 `get_cached_children()`（纯本地零往返）。collect 循环不动（上限/顺序/refs 编号逐字节保留，真机 diff 实证）；`ACTL_WALK_LEGACY=1` 回落逐节点（provider 兼容逃生舱 + A/B 对照组）。真机：记事本 find --name **615ms**（原始 5.7s——**~10×**）；子串搜索 0 命中探测同趟免费。
- **树扫描属性预取（性能轮）**：`tree_bounded` 缓存请求带上 Name/ControlType/AutomationId/ProcessId——walk 后的逐元素三连读从 N×3 次跨进程往返变为**零往返缓存读**。真机 A/B（记事本）：`find --name` 2.4-2.6s vs 5.7-7.0s——**~2.5×**。
- **find --id provider 侧快路径（[06](docs/06-design-blueprint.md)）**：单一精确条件走 Subtree FindAll + AutomationId 等值——单次跨进程调用替代全树遍历(真机:记事本 258ms vs 1146ms,重树分钟级→秒级);命中走 L4 同形态（`resolved_by:"provider-exact"`），无命中回落全树保契约。**--name/--role 子串语义不走此路径（UIA 属性条件仅等值——诚实边界）**。
- **进程看门狗默认降档 300s→120s + 心跳修真（[06 §7.7](docs/06-design-blueprint.md)）**：心跳从"循环前重置一次"（轮询超期限即误杀的潜伏缺陷）修为循环内逐轮/逐批，树扫描补**逐节点心跳**——合法长路径（全树扫描分钟级）全部带心跳不受降档影响，心跳停止的单点卡死才触顶。
- **lint-packs 选择器形态建议（知识层杠杆,advisory 不计失败）**：对 `name:` 步骤给提示——元素有稳定 AutomationId 优先 `id:`（provider 单调用快路径）。选择器优先级约定入 [PACKS.md](skills/actl/PACKS.md)。**不做无证据的批量改写**（先探测再写死纪律）。

### Fixed

- **前台快拒绝（[06 运行时治理⑦](docs/06-design-blueprint.md)——后台懒激活慢墙根因收口，release-review-11 两轮 runner 看门狗击杀实证）**：Word 类 provider 在锚窗非前台时懒激活节流——同命令实测前台 ~2s/后台 14-22.6s，type/press 的昂贵全树 locate 跑在守卫之前，单次 COM 调用撞 20s UIA 看门狗即击杀进程、flow 滞留 running。两处收口：① type/press 在 locate 前经廉价原生枚举核对锚窗前台态——非前台先两级激活（原生 SetForegroundWindow[最小化先还原]→锚窗单元素 UIA SetFocus[单次 COM 调用不做树遍历]），仍非前台 NOT_ACTIONABLE 让行（守卫语义同既有 set_focus+settle，拒绝点提前）；锚窗不存在不抢答；click 不设此门（语义 Invoke 后台窗合法）。② front-report 锚窗非前台时走原生快门：in_tree 置空+`in_tree_skipped` 原因、owned 原生层照报。真机实证：用户持前台时 front-report 20s+→1.2s、press 经 SetFocus 激活路径送达（零阵亡）；release-review-11 修复后写段零重试全绿。测试：foreground_gate 纯逻辑 4。
- **信号条文案与状态一致性（两批，[06 §7.5.5](docs/06-design-blueprint.md)——文案审计轮；新增信号条文案规范节）**：① reason 映射补 8 个枚举/前缀族——popup_pending/ambiguous_target/budget_exhausted/decision_pending/output_freshness_failed/observation_failed/goal_check_failed/no_candidates 此前全落"需要处理"泛化兜底，其中 popup_pending 在横条显示"待核对结果·可操作"属语义错配（正确动作是处理弹窗）；model 横条给三态专属短文案。② 术语分词：核验=机器（verifying→核验中）/核对=人（needs_review→待核对）。③ 停止族三处话术统一标准句；run 级 stopped→"流程已停止"与执行级"急停已启用"区分。④ 引擎 reason 收敛纯枚举（分号拼英文长句形态删除）。⑤ feedback 英文屏蔽放宽**中文直通** + status_name ready 死映射清理。新增 reason 必须同步 ui_text::reason 映射入交付清单。测试 +4。
- **尾段挂起族终局（两轮根治+根杀，[70 §4](docs/70-debug-playbook.md)）**：真机实证成功后进程不退出（CPU 恒零赖活数小时，内核态不可杀）——根因 = UIElement 的 Drop（跨进程 Release）与读取调用同族无界，析构路径无看门狗覆盖。三步收口：① **成功路径硬退出**——envelope 落盘 flush 后 `process::exit(0)`（析构链整个跳过；状态落盘不依赖 Drop：finish 显式调用、Journal 每事件即 flush）；② **COM 元素不出 actl-uia 域**——`ResolvedInfo` 元数据面 + `Located::into_info()` 消费式转换（read/verify/click/set_value/select/toggle/pointer/type 家族全收编）；③ **终杀**——finish 两分支改 hard_exit_after_envelope（watchdog 直调 TerminateProcess，绕过一切运行时清理），SignalSession::stop_worker 改 detach。真机实证：原卡死命令序列 808ms/429ms 干净退出、**list-windows 403 窗僵尸进程清零**；树扫描区段口径同步修为逐节点 rearm（"单次跨进程调用"的有界性——冷 provider 合法慢扫描不再被整段 20s 误杀）。配套 docs/70 §4 两条纪律。
- **看门狗 reset() 恒等式 P0（进展从不续命）**：watchdog::reset 的实现缺陷使进展点重置恒不生效——进程看门狗退化为纯总时长硬顶，合法长路径（全树扫描/长录制/慢沉降）会被误杀而"心跳修真"语义没有真正落地。修复后逐轮/逐批/逐节点心跳真正续命。
- **step 失败证据保留错误原文（[68 待办](docs/68-combined-batch-first-round.md) 收口）**：`failure_evidence` 的 metadata 分支统一附带有界 `message`（200 字符截断）——修复前**带** evidence 的失败反而只剩 code 丢原文，恢复路径必须重放才能看到原因。golden×4 同步 + 新增回归用例。
- **物化 provider 拒答自动回落 + 消费分轨（Word 边界结构性绕开，[06](docs/06-design-blueprint.md)）**：Word 类 provider 对 Subtree 缓存请求返回 Malformed Cacheresponse——此前整轮观察失败；现自动回落逐节点遍历（诚实慢不炸），且物化消费分轨：观察物化（可回落）与动作链恒 live（动作前必须新鲜身份）——Word 兼容模式文档全链由此可行（note-typo-012 evidence 指纹级全链实证）。
- **null 身份交互元素降级入树（[06](docs/06-design-blueprint.md)）**：无 RuntimeId 的交互元素（如 Word 任务窗格菜单项）此前在身份登记时炸整轮观察（INTERNAL）；现降级如实入树（identity-less 标注 + stderr 留痕），观察不再被单个缺陷元素中断。
- **Word 写段三处结构修复（skill，release-review-draft v4-v5 + word pack quirks，[43 §4](docs/43-release-review-draft.md)）**：① resources 复制 seed.docx + launch /n **带文件参数**结构性绕过本机「启动时显示开始屏幕」（裸 /n 停开始屏幕，深色主题下呈灰色，UIA 无 Document——release-review-09 实证）；② open_word verify 收紧为 `wait --element role:Document` 物化门；③ 另存为链锚定**对话框自身标题**（owned 32770 独立前台窗，主窗锚的前台守卫结构上无法对齐）+ 文件名框 uia 严格复合选择器 + popup_ignores 认领。真机：release-review-11 全链 11 步零重试，产物文件级核验通过。quirks 两条入 word.yml；word-note compatibilityMode 12→15 根因修复与 VS Code find 面板 wait --gone 断言撤销（真机否证 UIA 滞留永久）同轮。
- **scroll UIA 分支方向反转（真机核验通过）**：`scroll_element` 语义分支把 notches≥0（`--up`）映射到 SmallIncrement（UIA 语义=**向下**），与物理滚轮分支反向——`--up` 经 UIA 通道实际向下滚。修为 `--up`→SmallDecrement。真机：资源管理器列表 `scroll --down 5` 后物化窗口下移 4-5 条。物理分支本就正确。
- **读路径 UIA 看门狗缺口（真机实证:`get --property value` 挂起超 300s）**：`read_property` 与 `verify` 的 Value/Checked 分支补 `watchdog::enter()` 区段——定位覆盖止于 locate，属性/模式读取是最后的裸区段；探索引擎 goal 断言走同路径。click pattern 派发同族先行收编。
- **launch 选项后置静默失效**：trailing_var_arg 语义下程序名后的 token 原样转发——`launch <prog> --wait-window X` 中选项从未生效且子进程把选项当参数（记事本实证）；现 fail-fast PROTOCOL 带选项前置指引（只拦本命令独有 `--wait-window`）。契约测试 + help 同步。
- **launch App Paths 解析面补 HKCU**：Store 版应用（notepad.exe）只注册 HKCU，此前仅查 HKLM → NotFound；真机验证裸名解析成功；单测 volatile 键自注册自清理。
- **测试线程栈上限（clap 命令树深度，[.cargo/config.toml](.cargo/config.toml)）**：第 50 个子命令使 selector_input 全量解析测试越过 libtest 默认 2MB 栈——`RUST_MIN_STACK=8388608` 仓库级统一（真机二进制主线程栈充裕不受影响）。
- **handoff golden 契约脱环境依赖（主分支既有红项）**：`handoff::error` 改路径注入化（`error_at`），golden 补基建②欠下的 execution 字段同步——原测试直读真实 %LOCALAPPDATA% 代次，任何有执行状态的机器必红。

### Documentation and validation

- **note-typo-012 干净轮收口（Word 样式流程 evidence 二进制指纹级全链，[81 §10](docs/81-word-style-flow.md)）**：环境清零后 **10 步全链 completed，证据二进制 fnv128=19f4b6a8**；**默认行走模式**（物化观察+live 动作链，无 legacy 逃生舱）focus 无 NOT_ACTIONABLE——「Word 轮必走 ACTL_WALK_LEGACY」更新为默认模式已过全链。产物独立 meta 复验通过、Word 优雅关闭。
- **资源管理器路线 #4 三步闭环（探索→回写→重放全链，[77](docs/77-exploration-s2-acceptance.md)）**：后退导航探索 goal_met actions=1 一发命中→explore-writeback 抽片段→重放 verified attempts=1；**apps/explorer.yml 档案首建**（collect/按名查找双路线 verified，虚拟化实证入 quirks）；地址栏双歧义+goal 子串坑两发现入档。§7 路线积累 4/5。
- **L0 路线命中重放的 skill 层流程化（[77](docs/77-exploration-s2-acceptance.md) 遗留收口）**：SKILL.md「应用路线」节落地任务到达双分岔指引——命中 verified 路线直接重放（≥3 步转 flow/短路线 batch）/unverified 走探索引擎（决策卡应答+writeback 抽片段人工复核转 verified）——知识飞轮读侧闭环。
- **遗留验收三项收口（2026-10-05）**：① 健壮性基建第一轮真机验收（009 轮 10 步全链 155s，七件基建逐项生效）；② 录制值读回 #4 双轨收口（证据二进制正式轮 fnv128=519b005f）；③ 网易云队列「收起」方向复验——识图推翻「树内残留=未收起」误判（Electron UIA 树滞留）；frameless 程序化关闭全路径无效入 pack quirks，可行路径=像素定位 X + click xy --physical。
- **docs/06 全面重写为纯现行态设计蓝图 + "慢日"重定性**：尾部 18 个日期增补节（占全文 37%，与 CHANGELOG/git 三层冗余）全部归位属主章节，治理段收编编号条款；头部维护契约（只记现行态/禁止日期增补节）。"慢日"重定性为**自身污染**（僵尸累积+自身开销误计；"隔夜恢复"颗粒度匹配进程生命周期而非环境因素），慢现场处置协议（doctor --clean+协变量，干净现场复现才归环境）取代环境叙事；显示端 UIA 并发面审计证伪（零 UIA COM 占用）。

## [0.2.1] - 2026-10-05

本版是里程碑间的能力补丁：0.2.0 建成的执行底座上，补齐**「人演示→YAML」的录制入口**（GUI 操作录制四件）、**焦点元素寻址**（树不可见应用的单次注入打字）与**运行时治理**（失败采集、弹层门禁、修订续跑、幂等步骤、进程看门狗）；探索模式补完 S2/S3/L2 成闭环，Word 样式流程沉淀首条真实业务编排。四条真机验收轨全部通过，详见各 docs。

### Added


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。

- **popup_ignores 弹层门禁降噪第二轮（spec，[06 运行时治理](docs/06-design-blueprint.md)——两个独立应用同根因升级为类问题后的声明式收口）**：门禁观察面分不清「常驻部件」与「真弹窗」——Word 停靠导航窗格（fill-3 误拦）+ VS Code Find/Replace 面板（常驻 UIA 表面，关闭仅视觉折叠、带按钮挡一切写步=结构性死锁）两实例实证。**词表启发退役**（is_docked_pane 面板词白名单不泛化，第二实例即滑过）；**provider 可见性路线探针否证**（VS Code 折叠面板仍报 offscreen=false——Electron 对视觉折叠不说实话，如实入档）。正解=声明认领：popup_ignores: [{app, name_contains}]，门禁三处同口径跳过（步前判定/pending 过滤/处置后复查）；name_contains 必填（最窄形态，禁止掏空门禁的宽声明）；同表面 handler+ignore 双声明 PROTOCOL（处置与放行互斥）；与 handler 双层共存合法。Word 模板按 010 实证补认领（.docx+导航）。测试：popup_gate +4（认领放行/词不符仍拦/双层共存处置+认领两不相扰/schema 互斥拒绝）；uia 词表测试随退役删除。真机验收通过（2026-10-11，VS Code 主轮 v4 修订同现场 A/B）：v3 同现场被常驻面板死锁挡住的 find-replace/quick-open×2，认领声明后全部 completed+verified——同一现场的对照铁证；run 收于 25/26（close 步 precondition 阻断于操作者中途清窗导致的 bind 陈旧，非机制缺陷，手动等价收尾+进程零残留）。
- **GUI 操作录制：人演示一遍，得到可复放的 YAML（spec，[76](docs/76-gui-recording.md) 设计 / [79](docs/79-quicker-recording-research.md) 调研 / [06 录制节](docs/06-design-blueprint.md) 协议）**：`record-start/record-worker/record-stop/record-status` 四命令——低级键鼠钩子捕获（回调极简入队 + 工作线程点击瞬间 UIA 反查 + 焦点身份），trace JSONL 证据文件（密码框键 VK 永不落盘、注入事件如实标注不编译、自家窗口过滤）。**信号条/托盘右键「开始/停止录制」**（动态菜单项，label 前缀「录制中」满足反后台录制提示）。机械编译器：换窗延迟分析→`wait` 步骤；键入 burst 末**值读回**（提交文本覆盖 VK 推导——IME 候选/自动纠正的真值；中文自动 `--paste`）；`--app` 字面标题泛化为 flow inputs。agent 改写轮指引落 skills（[record-rewrite.md](skills/actl/references/record-rewrite.md)：TODO 分类修复/选择器稳健化/噪音清除/参数化/**修订优先迭代**）。真机验收：注入轨四发现全修（DPI 口径/LLKHF 位/Value→Text 回退/读回竞态），人工演示轨 47 事件 43 真人输入全捕获 + IME 旗舰断言成立，改写重放 v1→v7 六轮收敛至单命令零前置全链通过——[80](docs/80-record-acceptance.md) 全记录。
- **焦点元素寻址 `focused:`（spec，[06 §3 命名规则](docs/06-design-blueprint.md)）**：`focused:`（裸）/ `focused:<Type>`（角色校验）解析到 OS 键盘焦点元素——树对语义选择器不可见但焦点可达的应用（Electron 懒树，uTools/网易云实证）的唯一元素寻址通道。属主判据用物理焦点根（GUITHREADINFO，UIA focused 视图在 WinUI 会谎报），角色/属主不符 fail-closed NOT_FOUND 带证据。**与 type 组合即单次注入打字**：`type focused: <词> --app <win>` 走既有逐字键盘通道一次注入全词——uTools 实测 5 字符 916ms（逐键 press 五调用 ≈8.5s），识图实证搜索筛选被驱动；网易云中文 3 字符同验证。
- **运行时治理：失败可采集、弹窗先判读、修改可续跑（spec，[06 §7.7](docs/06-design-blueprint.md) 三轮）**：① **`flow-evidence` 失败采集包**——uncertain/门禁阻断瞬间自动归集截图 + 窗口清单 + 定向 UIA 快照 + 前台弹层快照（含分类），新只读命令索引；② **写步步前弹层门禁 + `popup_handlers` 声明**——写步动手前先听前台引导层，未知面板（unknown 且可交互）→ `popup_pending` 零副作用拦截；声明过分类的面板（error/save/confirm/update）经既有 click 全护栏自动处置留痕；Win11 对话框宿主空壳降噪（无按钮 unknown 不阻断）；③ **`flow-revise` 版本化修订续跑**——步骤级 diff，完成步规格未变保留结果（`plan_version` 溯源），写步变更须 `--accept-rerun` 点名，uncertain 步变更即重试授权；旧计划字节归档永不覆盖、版本谱系 flow-status 可见；④ **`ensure:` 幂等步骤**——只读 check 通过直接完成不派发（重跑/续跑环境前缀自动短路）；⑤ **进程看门狗**——「无进展」判定（步派发/等待轮询/恢复重试/录制事件/探索轮次为心跳），默认 300s（`ACTL_WATCHDOG_MS`），耗尽自我终止留 stderr 标记，UIA 无界挂起族（15 僵尸实证）从不可杀变为限内诚实失败。真机 E2E 全链：冲突框 popup_pending 拦截→四工件采集→修订 v2→自动处置→续跑 completed。附带 `retry_hint`（needs_review 信封直接给可复制的 `--retry-step` 命令）。
- **探索模式 S2/S3/L2 成闭环（spec，[42](docs/42-exploration-mode.md) / [06 探索节](docs/06-design-blueprint.md) / 真机 [77](docs/77-exploration-s2-acceptance.md)）**：**S2 有界探索引擎**——关键词投影观察→差分→前沿评分→熵门（头部领先自动执行、平局外抛决策卡 4KiB/Top-K≤8）→probe-before-act→点击归因→本地 goal 核验；`explore` 计划段直接 flow-run，`flow-resume --decision` 应答。**S3 路线回写**——`explore-writeback` 从完成运行抽取 pack 片段（status 恒 unverified，人工复核迁移），死候选生成 quirk、tried 数据导出 `explore.priors` 效果风险先验（先验飞轮实证：同 goal 从 `goal_check_failed` 变预算耗尽）。**L2 原语**——`explore.texts` 输入原语（解锁搜索框/文件名类任务）+ 跨窗 goal 探测（对话框内断言链）。真机：计算器/记事本 4 场景 + L2 + S3 闭环全过，token 审计实测关键词帧 0.3-1.7KiB/次。
- **Word 样式流程与文档断言扩展（[81](docs/81-word-style-flow.md)）**：`verify-file --kind docx-styles` 逐段样式断言（OOXML styleId + 加粗，机器可验不依赖视觉）；`--kind meta` 落盘元数据断言（`{size,mtime_unix_ms}` + `size_min/mtime_min` 关系键，**元数据读取不受 Office 排他锁影响**——补上「ctrl+s 实际未落盘但只读活文档验证照样通过」的缺口）；`--expected-file` 期望文件读（shell 引号/编码坑消灭）。`word-note-typography.yml` 首条真实业务编排：模板预置样式 + 光标式应用 + 填充式输入（模板 v2 十二空段，步骤 21→12、输入调用 40→25），启动错误框经 popup_handlers 引擎自动处置（治理首轮真机首战）；Word pack 增 style_choreography 路线与四条 quirk。
- **健壮性基建（spec，[06 增补](docs/06-design-blueprint.md)；[81 §8](docs/81-word-style-flow.md) 事故链收口）**：UIA 调用区段看门狗（`ACTL_UIA_WATCHDOG_MS` 默认 20s，超时 exit 125 + stderr 标记——无界 COM 挂起限内失败）；close-window 存活复核（投递后有界等待销毁，仍存活 NOT_ACTIONABLE 带同 pid 弹窗清单）；list-windows 前台兜底（provider 楔死漏窗时 Win32 直读回填）；wait --element 瞬态重试（观察瞬态按未出现继续轮询）；**进程内定位缓存**（同 (app,target) 命中以 RuntimeId+根窗口双复核替代全树 DFS——Word 级树实测 DFS 2-3s/次的大头消除，`resolved_by:"cache"` 如实标注，仅静态语义选择器可缓存）；StepRecord 逐步耗时（started_ms/finished_ms 性能画像）。
- **双循环二进制纪律工具化（[70 §8](docs/70-debug-playbook.md)）**：`cargo xtask dev`（诊断循环，增量 DEBUG 实测 ~4s）/ `cargo xtask evidence`（证据轮，RELEASE + FNV-128 指纹落 binary-log.txt）结构性分离；Makefile 精简为高频别名层。

### Fixed

- **尾段挂起族根杀（fix，[70 §4](docs/70-debug-playbook.md)——06 定位/06 主修复/08 新签名暴露的类级终局）**：13112 签名实证 process::exit 走 CRT 清理可能到不了 OS（flow-resume envelope 落盘后 CPU 恒零赖活数小时）——析构链上的 UIElement 跨进程 Release/thread_local 定位缓存/signal worker join 全是无界阻塞面。inish 两分支改 envelope flush 后 hard_exit_after_envelope（watchdog 新增：直接 TerminateProcess，绕过一切运行时清理——envelope 已 flush、状态已显式落盘，不依赖析构面）；SignalSession::stop_worker 改 detach（join 曾把 finish 卡在 envelope 打印之前=「空输出」形态）。真机实证：list-windows 403 窗僵尸进程清零。配套 docs/70 §4 两条纪律（envelope 先落盘再解析/二次楔死即换通道）。
- **录制值读回护栏按 HWND 比对（[80 发现 #11](docs/80-record-acceptance.md)）**：护栏曾按焦点窗**标题**精确比对——编辑改标题（脏星号）即必然失配，值事件系统性缺失；改用焦点根 HWND（稳定身份）。#4 原始形态（即时停止）正式轮复验通过。
- **input 绑定渲染与残留守卫（[80 发现 #6/#6b](docs/80-record-acceptance.md)）**：渲染层把 `{input: name}` 绑定加引号成字符串致替换不发生，run/verify 拿同一字面量会**自洽假通过**；双层修复（`arg_scalar` 原样发射 + 引擎 PROTOCOL 拒绝残留绑定）。
- **YAML 数字/布尔/null 标量加引号（[80 发现 #5](docs/80-record-acceptance.md)）**：`--timeout 8022` 裸渲染被解析为整数而 flow schema 拒绝；数字形态纯量强制引号。
- **投影快照 ref 编号口径（[06 增补](docs/06-design-blueprint.md)）**：@eN 改为全树 DFS 序数——修复前投影快照重编后与 `click @eN --snapshot` 回放计数错位，投影 ref 从未正确回放过；三口径（输出=身份=回放）对齐。
- **同进程并行 UIA 读回退串行（真机证伪）**：两个并行 find 必有一个 `E_FAIL`——CLI 移除并行分发，批量通道保持串行。

### Documentation and validation

- **录制四条真机验收轨全记录（[80](docs/80-record-acceptance.md)）**：注入轨（四发现修复+文本保真对照）、人工演示轨（IME 提交值编译旗舰断言）、改写重放轮（v1→v7，#7-#11 发现全收口，最终单命令零前置 12 步全链）、#4 沉降正式轮；事故教训同步入档（弹层判读纪律/误点纠正/复活循环破除）。
- **探索模式 S2/S3/L2 真机验收（[77](docs/77-exploration-s2-acceptance.md)）**：熵门自动路径、决策卡往返、诚实终态、token 审计、回写+先验飞轮、texts 输入原语、跨窗 goal。
- **Word 样式流程真机验收（[81](docs/81-word-style-flow.md)）**：note-typo 多轮（样式断言/提速 30%/popup_handlers 首战/健壮性轮事故链）。
- **应用档案扩充**：新增 uTools pack（粘贴死路/树不可见焦点可达/focused 输入通道，search_launch_app verified）；网易云 search/open_queue 复验（focused 单调用中文输入，manual→verified）；Word 增样式路线；lint-packs 12 packs 323 steps。

### Known Limitations

- 非开发机 US3 复现搁置（0.2.0 遗留，用户裁定）；机器到位后补验。
- 浏览器写入 uncertain 状态不跨进程持久化（0.2.0 遗留，归 M3）。
- 网易云队列「收起」方向未完全证实（展开已验；二次点击 ok 但 Table 树内残留，待复查）。
- 健壮性基建第一轮的真机验收部分挂起（[81 §8](docs/81-word-style-flow.md)：009 轮待环境恢复）；看门狗/弹层门禁/修订续跑/ensure 已有契约或 E2E 验证。
- `record-pause` 未实现（菜单不设项）；录制会话时长上限内单会话。

## [0.2.0] - 2026-10-03

本版是浏览器 CDP 通道 + 指针扩展 + 探索模式地基的版本：从零到一建成了 actl 自有浏览器自动化通道（9 个 spec 提交），三项指针扩展全部真机通过，探索模式 S1 落地；同时补齐本地应用注册表、帮助精简、AGENTS 工程纪律、显示端动画与 UIA 诊断增强。

### Added


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。

- **popup_ignores 弹层门禁降噪第二轮（spec，[06 运行时治理](docs/06-design-blueprint.md)——两个独立应用同根因升级为类问题后的声明式收口）**：门禁观察面分不清「常驻部件」与「真弹窗」——Word 停靠导航窗格（fill-3 误拦）+ VS Code Find/Replace 面板（常驻 UIA 表面，关闭仅视觉折叠、带按钮挡一切写步=结构性死锁）两实例实证。**词表启发退役**（is_docked_pane 面板词白名单不泛化，第二实例即滑过）；**provider 可见性路线探针否证**（VS Code 折叠面板仍报 offscreen=false——Electron 对视觉折叠不说实话，如实入档）。正解=声明认领：popup_ignores: [{app, name_contains}]，门禁三处同口径跳过（步前判定/pending 过滤/处置后复查）；name_contains 必填（最窄形态，禁止掏空门禁的宽声明）；同表面 handler+ignore 双声明 PROTOCOL（处置与放行互斥）；与 handler 双层共存合法。Word 模板按 010 实证补认领（.docx+导航）。测试：popup_gate +4（认领放行/词不符仍拦/双层共存处置+认领两不相扰/schema 互斥拒绝）；uia 词表测试随退役删除。真机验收通过（2026-10-11，VS Code 主轮 v4 修订同现场 A/B）：v3 同现场被常驻面板死锁挡住的 find-replace/quick-open×2，认领声明后全部 completed+verified——同一现场的对照铁证；run 收于 25/26（close 步 precondition 阻断于操作者中途清窗导致的 bind 陈旧，非机制缺陷，手动等价收尾+进程零残留）。
- **浏览器 CDP 通道 v1（`browser-*` 命令族，spec）**：actl 自有浏览器会话的最小同步 CDP 客户端（新 crate `actl-browser`，tungstenite 为唯一新增直接依赖）。**8 命令**：`browser-launch`（默认有头、`--headless` 后台选项、`--proxy` 显式代理、`--profile` 持久登录复用、`--browser-executable` 接受 app-registry 别名）、`browser-navigate`（网络级失败显式报错不静默）、`browser-read`（`--property` 白名单属性读 + `--iframe` 跨源子 frame）、`browser-type`（守卫输入写，native setter + 读回核验，内容不回显）、`browser-click`（守卫写 + `--expect-text` 断言）、`browser-close`（CDP 优雅关闭 + 幂等注销）、`browser-list`（只读枚举）、`browser-verify`（断言族 + 登录态谓词 `--css/--xpath [--contains]`）。**持久 profile 四件套**：登录暂停点（`pause_before`+`resume_check`）、`resources.browser_profiles` 声明治理（未声明即拒）、同名再启=复用+陈旧接管、扩展策略（ephemeral 恒禁/持久默认保留）。**YAML 流程接入**：治理三件套（outputs 只读生产者/副作用必带 verify/持久 profile 声明）全部就位，探针六步+登录暂停+复用真机全通。安全边界：Target/Runtime/Page/Browser 白名单域——cookie 构造性不可达，永不附着用户默认 profile。设计 [72](docs/72-browser-cdp-integration.md)；x.com 八轮真机 [73](docs/73-browser-twitter-scenario.md)。
- **本地应用注册表（`app-register`/`app-list`/`app-remove`）**：`%LOCALAPPDATA%/actl/apps.json` 存储名字→路径映射，`launch <别名>` 和 `browser-launch --browser-executable <别名>` 走注册表解析；Store 应用支持 `--pattern` 通配（版本号变化自动重解析 glob）。skills 文档含查找指南 [app-locations.md](skills/actl/references/app-locations.md)。
- **探索模式 S1（spec）**：`crates/actl-core/src/diff.rs` 多级身份配对差分 API（automation_id+role 优先、动态 id 回退 name+role、大小写不敏感、重复键容忍——虚拟化列表场景全覆盖，6 单测全绿）；`Projection::Keyword` 任务关键词投影（命中交互元素+祖先链，零命中=空不回退）；`snapshot --keywords K1,K2` CLI 面；devtools `snapshot-diff` 升级为 core 配对。
- **指针扩展（M3 P3，spec）**：①轨迹拖拽 `drag --path X,Y`（≤64 路径点 8px 插值）；②坐标框选 `drag xy:x1,y1 xy:x2,y2 --app`（两端窗口边界+遮挡守卫）；③长按 `key-down --duration Ms`（急停即释放并 ABORTED）。Paint 真机三项全通，见 [74](docs/74-pointer-extensions.md)。
- **动作开始中心闪现（`action_flash`）**：agent 动作启动物理/CDP 写时，屏幕居中偏上出现单行胶囊（`ACTL | 动作名`），从左滑入→停留 1.6s→向右滑出，穿透+置顶+不激活；与信号条同色系。
- **中央任务起止提示**：只在任务开始、完成及异常/中断时提示，步骤过程沿用原状态条；B 开放光带样式，横向从工作区三分之一处进入、中央停留、向三分之二处退出。见 [75](docs/75-action-start-notification.md)。
- **stdio 继承守卫（`actl-uia::process_io`）**：浏览器进程树会继承调用方管道写端致 `$()`/`ReadToEnd` 永不 EOF——长命子进程派生前清自身 stdio 的 INHERIT 标志。兜底一切分离式子进程派生。
- **探索模式 spec 评审通过（[42 §9](docs/42-exploration-mode.md)）**：PRD 非目标逐条核对通过；协议 delta 定稿（NeedsReview 扩展、决策卡 4KiB/Top-K≤8、`flow-resume --decision`）；S1 先行。
- **GUI 操作录制设计提案（[76](docs/76-gui-recording.md)）**：用户手动操作→自动生成 YAML（像 RPA recorder）；~800-1200 行，0.2.0 后优先立项。

### Changed

- **帮助输出精简**：18 个命令的 selector_help 重复块改为 `"see actl find --help"` 引用；`--no-activate` 从 ~200→~60 字符；`--physical` 从 ~130→~40 字符；snapshot 六选项单行化。总体削减 ~8000 字符。
- **skills 文档全面更新**：SKILL.md 新增浏览器三通道路由（playwright/actl CDP/actl UIA）+ 命令速查新增浏览器/app-registry 分类；flows.md 新增浏览器流程规则五条（outputs 限制/verify 断言族/登录暂停/值断言/profile 声明）；版本号从 0.1.6→≥0.1.9。
- **显示端 UIA 响应**：修复检测和只读探测移出窗口消息线程；屏幕边缘提示复用 HWND 减少重建。见 [65](docs/65-observation-blocking-diagnosis.md)。
- **历史日志保留策略（spec）**：检查间隔 1h→15d，容量 64MiB→3GiB；跨进程节流。
- **UIA 查询阶段诊断（spec）**：children 查询细分 FindAllBuildCache/Length/转换，零结果与未知区分。
- **UIA 窗口观察优化**：单次观察内复用客户端和缓存请求，减少重复 COM 调用。
- **任务时间线查询**：按有效首条筛选，有界任务头缓存跳过未变化 journal。
- **UIA 观察诊断（spec）**：`UIA_E_TIMEOUT` 映射为 `TIMEOUT`；观察阶段首尾/耗时/错误码/前台变化/输入活动摘要入时间线。
- **历史清理扫描性能**：仅在超预算时读取排序；按需 trace 增加耗时汇总。
- **桌面根诊断**：有界 UIA 桌面根身份探针，缓存模式须显式指定。

### Known Limitations

- 非开发机 US3 复现搁置（用户裁定 2026-10-02）；机器到位后补验。
- Store 应用 app-registry pattern 受 WindowsApps 权限限制：glob 无法枚举该目录，需注册完整路径（更新后重新注册）。
- 浏览器写入的 uncertain 状态不跨进程持久化（崩溃后由调用方 browser-read 裁决）；flow 级 uncertain 持久化归 M3。
- 显示端常驻会话卡（区别于动作流的浏览器会话 UI）待后续。

## [0.1.9] - 2026-10-02

本版是可靠性、验收与安全加固的版本：金任务跨日双百达标，全部原生夹具和四场景真机复验通过，输入护栏与显示协议健壮化（前台沉降/租约重读/换行折叠/悬浮卡不抢前台），前台弹层优先体系落地（front-report + 判定树 + 清理白名单），授权通道定案（agent 不可自授、急停压过预授权）。

### Added


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。

- **popup_ignores 弹层门禁降噪第二轮（spec，[06 运行时治理](docs/06-design-blueprint.md)——两个独立应用同根因升级为类问题后的声明式收口）**：门禁观察面分不清「常驻部件」与「真弹窗」——Word 停靠导航窗格（fill-3 误拦）+ VS Code Find/Replace 面板（常驻 UIA 表面，关闭仅视觉折叠、带按钮挡一切写步=结构性死锁）两实例实证。**词表启发退役**（is_docked_pane 面板词白名单不泛化，第二实例即滑过）；**provider 可见性路线探针否证**（VS Code 折叠面板仍报 offscreen=false——Electron 对视觉折叠不说实话，如实入档）。正解=声明认领：popup_ignores: [{app, name_contains}]，门禁三处同口径跳过（步前判定/pending 过滤/处置后复查）；name_contains 必填（最窄形态，禁止掏空门禁的宽声明）；同表面 handler+ignore 双声明 PROTOCOL（处置与放行互斥）；与 handler 双层共存合法。Word 模板按 010 实证补认领（.docx+导航）。测试：popup_gate +4（认领放行/词不符仍拦/双层共存处置+认领两不相扰/schema 互斥拒绝）；uia 词表测试随退役删除。真机验收通过（2026-10-11，VS Code 主轮 v4 修订同现场 A/B）：v3 同现场被常驻面板死锁挡住的 find-replace/quick-open×2，认领声明后全部 completed+verified——同一现场的对照铁证；run 收于 25/26（close 步 precondition 阻断于操作者中途清窗导致的 bind 陈旧，非机制缺陷，手动等价收尾+进程零残留）。
- **始终信任此流程（指纹持久信任）**：授权三部曲收官——全局托盘开关 → 会话级 standing → **指纹持久信任**（跨会话免点击）。声明 `approval: standing` 的流程运行时，执行器计算冻结计划的 FNV-1a 双通道 128 位指纹并附在交接请求中；显示端查 `trusted-flows.json` 信任集——匹配即刻 Standing 授予，不匹配正常出卡。**模板变更 → 指纹变 → 信任自动失效**。设计见 [32](docs/32-desktop-handoff.md)。
- **`front-report` 前台弹层报告（只读）**：应用用最前方的弹窗引导用户——自动化动手前必须先听它说。汇聚原生 owned 可见弹窗与树内子窗表面（文本全文+按钮清单），附朴素分类提示（error/save/confirm/update/unknown），`front_clear:true` 表示无引导层。观察三件套齐备：title-watch / focus-report / front-report。
- **`focus-report` 只读诊断命令**：UIA 焦点元素、物理焦点根窗（GetGUIThreadInfo）、物理前台窗三视图并列——WinUI 上 UIA focused element 实测会谎报，物理层权威。
- **consent_decision 一等字段 `input_source`**：决策时刻最近输入来源分类（device/foreign_injected/own_injection + age_ms）——人工物理输入与外来程序注入一眼可辨。
- **verify-owner 进程树归属**：属主进程命令行不含令牌时沿父链（有界 4 级）找携带令牌的祖先，载荷带 `via_parent` 如实说明。
- **verify-file IO 失败可区分报错**：`io_kind`（sharing_violation/permission_denied/other）+ `os_code` 入 evidence。
- **flow-status 陈旧计划提示**：冻结计划旁车记录源模板字节指纹，重读比对不一致时提示"模板已改，重试仍跑旧路线"。
- **`flow-clean` 批量归档**：归档历史非完结流程（默认 60 分钟阈值），证据全保留。
- **诊断动词家族（devtools）**：`cleanup-tabs`（按名关标签+弹层白名单）、`snapshot-diff`（快照结构化差异）、`title-watch`（标题瞬态监视）、`close-gone`（关闭→消失计时）、`app-lab`（隔离应用实验台）、`focus-report`、`front-report`——ad-hoc 脚本全部沉淀入库。
- **`composite-locate` 夹具**：UIA 组合定位真机验收五点（复合唯一化/同名消歧/作用域/重建后重定位/暂停恢复后重定位）。

### Changed

- **excel-table-exit 进程断言按已知限制收口**：两个 Excel 退出步 verify 改为 `verify-window absent`（VSTO+输入法加载项钉住进程使进程断言结构性不满足；窗口关闭是可达的最强断言）。
- **preview 框语义澄清**：黄框 = 自动化操作矩形（`--physical` 点击落点为其中心），与视觉边框的偏差是风险信号而非渲染缺陷。

### Fixed

- **尾段挂起族根杀（fix，[70 §4](docs/70-debug-playbook.md)——06 定位/06 主修复/08 新签名暴露的类级终局）**：13112 签名实证 process::exit 走 CRT 清理可能到不了 OS（flow-resume envelope 落盘后 CPU 恒零赖活数小时）——析构链上的 UIElement 跨进程 Release/thread_local 定位缓存/signal worker join 全是无界阻塞面。inish 两分支改 envelope flush 后 hard_exit_after_envelope（watchdog 新增：直接 TerminateProcess，绕过一切运行时清理——envelope 已 flush、状态已显式落盘，不依赖析构面）；SignalSession::stop_worker 改 detach（join 曾把 finish 卡在 envelope 打印之前=「空输出」形态）。真机实证：list-windows 403 窗僵尸进程清零。配套 docs/70 §4 两条纪律（envelope 先落盘再解析/二次楔死即换通道）。
- **前台护栏激活过渡误拒**：type/press/paste 物理前台闸门改为有界沉降轮询（新旋钮 `foreground_settle_ms` 600ms）——激活过渡期空标题窗不再误判为抢焦。
- **显示租约有界重读**：check_active 改为 1500ms 预算内重读——租约新鲜度窗与渲染 tick 贴近导致截图捕获/DWM 打嗝时误报"显示不可用"。
- **`type --verify-value` 换行表示误判**：回读比较前 `\n`/`\r`/`\r\n` 统一折叠为 `\n`——Store 记事本 ValuePattern 实测报孤立 `\r`，严格字节比较必误报。
- **`type --verify-value` 失败证据补齐**：新增 `expected`/`actual`/`comparison`——差异本体首份信封可见。
- **悬浮卡不再抢前台**：横条悬浮信息卡改 `SW_SHOWNOACTIVATE` + `WS_EX_NOACTIVATE`——此前 `SetForegroundWindow` 会偷走自动化目标前台。
- **UWP 标题路径类配对**：同标题多窗时 `focus-window`/`restore-window`/`close-window` 限定 `ApplicationFrameWindow` 框架窗。
- **`${run.dir}` 路径展开**：input default 的路径展开不再依赖 resources 声明——无资源模板中原样传给命令致 screenshot 路径不存在。

### Fixed（夹具，devtools）

- **金任务 2 全链修通**（Store 记事本连串根因）：启动走执行别名、收尾按标签清理+弹层白名单、另存为文件名改键盘通道+阶段化有界重试、UWP 判定收窄、多标签纪律。
- **金任务 1 修通**（UWP 计算器归属唯一性）：owner() 新增 class 过滤限定 ApplicationFrameWindow。
- **夹具收尾双路线分流**：有多任务/同族标签走标签清理，无标签应用走整窗优雅关闭。

### Documentation and validation

- **金任务跨日双百**：GT1 10/10 + GT2 10/10（2026-10-01 晚间）。
- **NFR 数据**：冷启动 P50=67ms/P95=94ms、snapshot P50=433ms、输出体积 <2KB、skeleton 削减 45%。
- **发布产物核对**：gate 全绿、5.3MB 零依赖、79 契约测试 94 全过、golden 22 文件。
- **全部原生夹具复验通过**（11 项）：金 1/金 2/input-target/msinfo32/Explorer/显示端×2/Edge×2/Chrome/flow-evaluation。
- **场景轨四场景真机复验通过**：app-launch-observe、cloudmusic-playback、quark-search（含两处版本漂移修正）、tencent-meeting-draft。
- **修复检测/auto_resume 全链真机验收**：两判定+预算+急停阻断。
- **once_per_run / 组内暂停 / 任务隔离真机验收**（用户协作轮）。
- **stale_read/resume_check 真机矩阵补全**（四格收齐）。
- **有界只读恢复六判据真机验收**。
- **条件模板写入侧三模板真机验收**。
- **agent 代操作通道定案**（docs/32）：四条合成通道全部无法授予——授权必须真人。
- **预授权下急停仍生效（显式验证）**：standing 开启 → 热停 → 写被拒 → recover → 恢复正常。
- **无干预四软件整跑达成（第八轮补记）**：70/71 completed、零人工救场。

## [0.1.8] - 2026-09-30

本版是显示端与人机协作体验的大版本：显示协议演进到 v11（预授权 Standing + 有界等待让行），横条信息架构重做（两行展开/悬浮卡/幽灵按钮/步骤链），暂停-修复-继续链路全程"说真话"（上下文按钮语义、障碍提示、修复探测、opt-in 自动续跑），并修复一个输入安全缺陷。

### Changed

- **[BC] 显示协议 v10–v11，`actl` 与 `actl-signal` 需成对升级**：新增 `Approval::Standing` 常驻信任——托盘开关「预授权」开启后桌面写入免出卡（横条青色描边标记，关闭即撤销）；流程声明 `approval: standing` 首次确认后会话内常驻。v11 起 CHECK 区分两种让行：授权仍在、仅输入活跃 → `YIELD_BUSY`，客户端有界等待安静（3 秒/150ms 轮询，急停即时），超时才失败——误触鼠标/键盘不再打断任务；授权真被撤销维持 YIELDED 立即失败。审计只记终态。[32](docs/32-desktop-handoff.md)
- **横条信息架构重做（四轮用户反馈迭代）**：点击空白展开为 **420×62 两行态**（宽度 1.5 倍、右缘锚定、无箭头指示、再点收回不自动收回），第二行显示当前工作流**步骤链**（✓ 完成/▸ 进行/✕ 失败/○ 待执行/◇ 跳过）；边框成为独立通道（预授权=青色常驻、进行中=状态色慢呼吸、需决策=快脉冲、静止无动画）；**悬浮信息卡**（dwell 500ms）：状态点=图例、空白区=一站式状态卡、步骤链=完整未截断值，与菜单/详情弹窗互斥且带归属。任务详情窗口：标题与状态同行、底部操作单行、幽灵描边按钮、TypeScript/WebView2 内容层（运行时不可用回退原生）。[18](docs/18-display-layer-implementation.md)
- **暂停-修复-继续链路（批次 1–4）**：继续按钮按上下文差异化（失败暂停=「已修复，重试继续」、待核对=「核验并继续」）；横条失败态带步骤 id；详情给**障碍提示**（错误码→可核实的障碍）、尝试计数、留证目录直达；暂停请求可见化（「正在暂停 · {步骤}」+ 超时急停教育）；type 分段间隙观察暂停（长输入亚秒级停下、已送达段数入证据）；action_failed 暂停期间显示端每 5 秒**静默探测** before/verify 并点亮判定（障碍已清除/结果已存在-防代做双重生效）；流程 YAML 新增 `auto_resume: action_failed`（默认关）：Retriable 判定自动续跑，预算 2 次/运行持久计数，急停/路线缺陷永不自动。[32](docs/32-desktop-handoff.md)

### Fixed

- **尾段挂起族根杀（fix，[70 §4](docs/70-debug-playbook.md)——06 定位/06 主修复/08 新签名暴露的类级终局）**：13112 签名实证 process::exit 走 CRT 清理可能到不了 OS（flow-resume envelope 落盘后 CPU 恒零赖活数小时）——析构链上的 UIElement 跨进程 Release/thread_local 定位缓存/signal worker join 全是无界阻塞面。inish 两分支改 envelope flush 后 hard_exit_after_envelope（watchdog 新增：直接 TerminateProcess，绕过一切运行时清理——envelope 已 flush、状态已显式落盘，不依赖析构面）；SignalSession::stop_worker 改 detach（join 曾把 finish 卡在 envelope 打印之前=「空输出」形态）。真机实证：list-windows 403 窗僵尸进程清零。配套 docs/70 §4 两条纪律（envelope 先落盘再解析/二次楔死即换通道）。
- **`--physical --with` 修饰键静默误配 Win 键（安全）**：白名单收紧为 ctrl/shift/alt/win，未知键名 PROTOCOL 拒绝（三条编译路径），"win"改为显式映射——此前 `--with ctrlx` 会注入 Win+点击。
- **needs_review 不再永久钉住横条**：人工意图态（paused/stopped）持久保留继续入口；needs_review 30 分钟超时回空闲（状态文件与核对入口不变）。
- **修复检测真机验收修正**：auto_resume 派发传真实停止代次（曾传标记串致子进程即败）、预算持久计数防目标切换重置、探测证据落 `flows/<id>/probe.jsonl`、孤儿化暂停流程回落到给予窗口内最新 action_failed 目标。

### Documentation and validation

- 新增调试手册（docs/70）与三份横切参考文档（FALLBACK 兜底清单 / STABILITY 护栏地图 / PARAMETERS 时序参数总览）；timing 画像/扫描夹具原生落地（`cargo xtask timing profile|sweep`，三个默认参数本地再证真）；零点击扫描真机完成。批次 1–4 与显示端重做经 computer-use 驱动的真机验收（三组场景 + 三个缺陷当场修复）；预授权两条通道、glyph 渲染与呼吸观感的人工确认仍待用户。

## [0.1.7] - 2026-09-30

本版把四软件应用验证整体迁移为原生 YAML 流程并整批跑通，扩展流程协议（简单条件、有界恢复预算、失败隔离、声明式输入与资源），显示协议演进到 v9（停止代次、单次交接、前台资格转交），工具链全原生化（移除全部 PowerShell），并交付 timing 画像/扫描夹具与三份横切参考文档。

### Changed

- **[BC] 显示协议 v7–v9，`actl` 与 `actl-signal` 需成对升级**：急停废止已运行调用的代次，修复归档原始状态、不授予权限、不自动续跑（删除 v1/时间戳兼容解析与直接解除握手）；任务配置支持 `handoff.approval: once_per_run`，首次确认后同任务不因空闲/总时长反复交接；动作开始向前台资格进程转交、结束归还，覆盖连续 CLI、最小化与失败路径（身份核验接受同文件路径别名）。输入监听增加 Raw Input 冗余与消息泵心跳，健康未知即撤销授权。新增 `execution-status` / `execution-recover`；显示端定时核对停止代次并主动收回旧提示。[停止与恢复](docs/47-execution-recovery.md) · [交接策略](docs/52-task-handoff-policy.md) · [前台资格](docs/62-foreground-handoff.md)
- **[BC] 观察失败契约收紧**：窗口清单、身份基线、新窗口等待及 PID 定位迁移到可失败的 RawView 枚举，必要属性失败不再伪装为空值、缺席或唯一结果；`wait --idle` 系统输入查询失败返回带证据的 INTERNAL，并在轮询中响应流程暂停/停止。
- **工具链全原生化**：移除剩余 41 个 PowerShell 文件——业务动作迁入 workflow，诊断、回归调度、夹具与发布打包迁入 `actl-devtools`（经 `cargo xtask` 调用），门禁改跑 Rust 测试；verify-file/owner/closed 改用原生 ZIP/XML/Win32/WMI 检查器，删除运行时解释器，补 DTD/膨胀限制与进程身份测试；新增 `verify --url-query` 精确解码断言。[迁移记录](docs/61-native-tooling-migration.md)
- **Excel 关闭路线定案（模板）**：close 步改后台文档关闭序列（文件菜单 → ListItem「关闭」→ 等后台消失），断言保持工作簿窗口消失（`wait --gone`，60 秒拆窗耐心）；close-window 新增 `--system-close`（投递 SC_CLOSE，点标题栏 X 的同一命令）——Excel 裸框架对 WM_CLOSE 只隐藏不退程，模板在后台关闭后以 `close-window --hwnd <归属> --system-close` + `verify-closed`（窗口+进程双断言）收尾。完整流程后 VSTO/输入法加载项钉住进程为已知限制，壳归环境治理。[docs/68](docs/68-combined-batch-first-round.md)
- **观察耐心家族收口**：verify-window 断言失败改 15 秒内重读（带加载项的 Excel 拆窗实测 3–15 秒）；`verify --value` 与 verify-clipboard 补 5 秒沉降；verify-rows 行树缺失改报 NOT_FOUND 入沉降（尚未渲染，可重试）；`wait --gone` 把垂死窗口的观察错误视为"尚未消失"在超时内继续轮询；有界恢复的 retryable 集纳入 INTERNAL（冷启动 UIA 抖动）。观察假阴性不再停整轮。[docs/68](docs/68-combined-batch-first-round.md) · [docs/70](docs/70-debug-playbook.md)
- **模板验证收敛化与就绪门**：易瞬态验证步配置 `recovery`（3 次/15 秒）只读自动重查；应用模板必填 `ready_target`，就绪阶段等待业务控件出现并稳定；组合模板首步 `wait --idle` 桌面空闲门。启动成功不再直接假定页面可输入。
- **UIA 语义动作执行前复核**：预览后按 RuntimeId、PID、角色、名称、AutomationId 与原生宿主身份复核目标，布局移动不再误报不可操作；控件禁用或身份变化仍拒绝，键盘焦点与物理命中护栏保留。
- **标题定位原生优先**：先从可见原生窗口筛选候选、仅读取候选的 UIA provider 并复核 PID 与标题，不存在原生候选时保留虚拟窗口发现路径——已知窗口的定位不再被无关应用的整桌面枚举故障阻断。
- **窗口激活去重**：已在前台不重复请求激活；后台使用单次原生前台请求并有界核验；启动与前台步骤拆分，避免已启动窗口因激活失败被重复启动。
- **内部时序一致性**：desktop-prepare/restore 与 minimize/restore 的输入锁等待、挂起唤醒再探针、verify 族沉降轮询全部改跟 `Timing` 旋钮（`ACTL_LOCK_WAIT_MS`/`ACTL_PROBE_MS`/`ACTL_POLL_MS`），默认值不变，仅 env 联动生效。[PARAMETERS](docs/PARAMETERS.md)
- **维护性**：历史清理间隔 60 秒→60 分钟并移入显示端后台线程；旧契约用例合并去重、并发测试写者循环缩减；gate 各步耗时随 stderr 输出；陈旧 PowerShell 描述与注释清理。

### Added


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。

- **popup_ignores 弹层门禁降噪第二轮（spec，[06 运行时治理](docs/06-design-blueprint.md)——两个独立应用同根因升级为类问题后的声明式收口）**：门禁观察面分不清「常驻部件」与「真弹窗」——Word 停靠导航窗格（fill-3 误拦）+ VS Code Find/Replace 面板（常驻 UIA 表面，关闭仅视觉折叠、带按钮挡一切写步=结构性死锁）两实例实证。**词表启发退役**（is_docked_pane 面板词白名单不泛化，第二实例即滑过）；**provider 可见性路线探针否证**（VS Code 折叠面板仍报 offscreen=false——Electron 对视觉折叠不说实话，如实入档）。正解=声明认领：popup_ignores: [{app, name_contains}]，门禁三处同口径跳过（步前判定/pending 过滤/处置后复查）；name_contains 必填（最窄形态，禁止掏空门禁的宽声明）；同表面 handler+ignore 双声明 PROTOCOL（处置与放行互斥）；与 handler 双层共存合法。Word 模板按 010 实证补认领（.docx+导航）。测试：popup_gate +4（认领放行/词不符仍拦/双层共存处置+认领两不相扰/schema 互斥拒绝）；uia 词表测试随退役删除。真机验收通过（2026-10-11，VS Code 主轮 v4 修订同现场 A/B）：v3 同现场被常驻面板死锁挡住的 find-replace/quick-open×2，认领声明后全部 completed+verified——同一现场的对照铁证；run 收于 25/26（close 步 precondition 阻断于操作者中途清窗导致的 bind 陈旧，非机制缺陷，手动等价收尾+进程零残留）。
- **流程协议扩展**：简单条件 `when/unless/otherwise`（条件观察失败不走否定分支，附参数化模板）；步骤级 `recovery` 有界观察恢复预算（持久化、继续不补满、暂停摘要含失败原因与重查日志）；`section` + `on_failure: skip_section` 失败隔离——一段失败保留证据、同段其余步骤跳过、后续段继续，整轮以 needs_review 收束，显式重试成功后还原；声明式 `inputs`/`--inputs` 与运行上下文资源（预检不创建、失败保留不覆盖、恢复不重初始化）；可恢复操作组（1–16 调用、以 verify 收尾）与文件/行集合/剪贴板/进程归属/关闭断言族；`${run.dir_native}` 反斜杠占位符（Windows 原生对话框拒收正斜杠路径）。[20](docs/20-yaml-workflow-protocol.md) · [63](docs/63-simple-workflow-conditions.md) · [64](docs/64-bounded-observation-recovery.md) · [55](docs/55-resumable-desktop-tasks.md) · [60](docs/60-workflow-resources.md)
- **失败分级与三路留证**：步骤记录 `attempt_errors`（最近 8 次错误码），同写步骤同错三连标记 `route_defect_suspected` 提示勿盲目重试；步骤失败瞬间向 flow-resources 落三件套——整屏 PNG、原生窗口清单 JSON、定向 UIA 快照 JSON（吞错不碍流程、同名覆盖不堆积）；无源失败合成带错误原文（截断 200 字）的最小结构化证据。
- **误触自动恢复（pardon）**：授权被纯鼠标移动类外部输入撤销时，静默窗（默认 4 秒）内无新输入且前台未变，显示端按每任务预算（默认 2 次）自动重新授权，满足同样条件的暂停流程自动发起续跑；任何键盘事件、点击、滚轮永不赦免，用户暂停、检查点、待核对与停止状态永不自动继续；YAML `handoff.pardon_input` 逐流程覆盖，`ACTL_HANDOFF_PARDON=0` 全局停用。真机模拟误触验收待做。[32](docs/32-desktop-handoff.md)
- **局部观察与目标预览**：`snapshot --within <严格选择器>` 容器局部快照（depth/limit、真实截断原因，局部 ref 绑定容器身份重放，不回退全窗口序号）；严格选择器含精确 ID/名称时先原生条件查询完整候选再核验唯一性（深树容器不被无关分支预算阻断）；`preview` 独立目标预览与 `--candidate` 同快照候选显式切换（不点击、不授予桌面权限、不默认选择），附预览失效交互回归用例。[48](docs/48-local-snapshot.md) · [50](docs/50-deep-container-query.md) · [46](docs/46-target-preview.md)
- **应用 workflow 全家**：VS Code/Word/Excel/Zotero 验证直接 flow-run，整批入口 verify-applications.yml（声明式隔离目录、初始文本与随包 Office 模板）；新增网易云播放、夸克搜索、腾讯会议草稿/日期、通用启动观察 workflow（once_per_run 交接，恢复先核验结果）；VS Code 改状态栏按钮与菜单控件路线，动态输入等稳定后再核验；首条业务流程 release-review-draft（Chrome→Word 全静态锚点，写段联测待授权）。[57](docs/57-application-workflow-migration.md) · [61](docs/61-native-tooling-migration.md) · [67](docs/67-vscode-navigation-controls.md) · [43](docs/43-release-review-draft.md)
- **诊断与回归入口**：`history --timeline` 把任务全部 journal 事件合并为单一时间序列；显示端触发的执行器经 `ACTL_TRACE_FILE` 落阶段 trace；`cargo xtask regress --unattended` 只跑登记的无输入用例；`cargo xtask preview-tests` 串行运行目标预览交互用例；`cargo xtask timing profile|sweep` 时序画像/参数扫描夹具——`ACTL_*` 旋钮的固定消费者，优雅关闭+verify-closed、标记文件隔离、急停/显示端未就绪/锁屏预检让行报阻塞（77）。[41](docs/41-logging-metadata.md) · [timing](docs/timing-2026-09.md)

### Fixed

- **尾段挂起族根杀（fix，[70 §4](docs/70-debug-playbook.md)——06 定位/06 主修复/08 新签名暴露的类级终局）**：13112 签名实证 process::exit 走 CRT 清理可能到不了 OS（flow-resume envelope 落盘后 CPU 恒零赖活数小时）——析构链上的 UIElement 跨进程 Release/thread_local 定位缓存/signal worker join 全是无界阻塞面。inish 两分支改 envelope flush 后 hard_exit_after_envelope（watchdog 新增：直接 TerminateProcess，绕过一切运行时清理——envelope 已 flush、状态已显式落盘，不依赖析构面）；SignalSession::stop_worker 改 detach（join 曾把 finish 卡在 envelope 打印之前=「空输出」形态）。真机实证：list-windows 403 窗僵尸进程清零。配套 docs/70 §4 两条纪律（envelope 先落盘再解析/二次楔死即换通道）。
- **整屏留证 COM 线程污染**：失败留证的整屏 PNG 路径把执行器线程 CoInitializeEx(STA) 后从不释放，同线程后续一切 UIA 调用以 RPC_E_CHANGED_MODE 失败——combined-batch 恢复核验连续 INTERNAL 的根因；改 COINIT_MULTITHREADED + 配对释放 + CHANGED_MODE 容忍。修复后整批 69/69。
- **桌面根枚举瞬态毒化**：标题定位的桌面根整批枚举被无关顶层窗口创建/销毁竞态毒化，所有应用报 STALE_REF、无匹配时误报；改仅对桌面根做有界瞬态重试（StaleRef、3 次/200ms），`uia:` 作用域枚举失败统一映射。净环境 8/8、扰动下直查 36/36。[38](docs/38-flow-output-bindings.md)
- **四软件路线缺陷（docs/68 定案）**：Word 打开期间正文核验改 UIA `role:Document` text（排他句柄期不做磁盘读，重开后兼查）；Excel 复制改 `type --verify-value` 提交跳转 + 窗口级 ctrl+c（网格单元格无元素级焦点上报）；Zotero 导入前置集合树就绪检查、快速搜索改 click 聚焦+全选+替换（type 为追加语义、tab 会清空过滤）、导出对话框移除 verify-owner（子进程无任务令牌）、导出文件名用 `run.dir_native`。Zotero 9/9、Excel 13/13 真机通过。
- **重复交接误判**：Raw Input 读取并校验事件来源，排除无设备且携带注入标记的自身事件；真实设备、未知来源与读取失败仍触发让行；交接历史记录弹窗、确认与失效原因，不记录键值/坐标。[54](docs/54-repeated-handoff.md)
- **显示与恢复细节**：悬停暂停/继续按钮不再覆盖主状态文字；pending 步骤恢复复用紧邻且完全相同的现场/前置检查；授权撤销保留首个原因，交接诊断关联失败调用与监听状态；display-status 过滤已退出进程的陈旧 popup 记录；状态条持久保留 flow 绑定，快照到期/显示端重启后仍可继续。
- **VS Code Quick Input 导航**：直接写 ValuePattern 并在 quickInput_list 内激活唯一结果；关闭核验只读进程实例身份。[59](docs/59-vscode-quick-input.md)
- **发布包技能补齐**：ZIP 包含可独立安装的 `skills/actl/`（应用档案、流程索引、references 本地相对链接），打包后核对必需文件与内部链接；不修改已发布的 0.1.6 附件。

### Documentation and validation

- 新增调试手册（调试阶梯、观察耐心对照、正斜杠原生表面规则、症状速查）与三份横切参考文档——FALLBACK（全量兜底/降级清单）、STABILITY（按步骤生命周期的护栏地图）、PARAMETERS（协议常量/内部旋钮/硬编码时序总览）；docs 索引同步。[70](docs/70-debug-playbook.md) · [FALLBACK](docs/FALLBACK.md) · [STABILITY](docs/STABILITY.md) · [PARAMETERS](docs/PARAMETERS.md)
- 十份应用 pack 同步输入身份规范（ref 快照绑定、焦点断言），修改过的已验证路线改记 manual 待复验；ZCode 超深会话与局部观察只读档案、无人值守验收工具留证。[45](docs/45-app-pack-refresh.md) · [49](docs/49-local-observation-acceptance.md) · [44](docs/44-phase2-acceptance.md)
- 真机状态如实记录：四软件整批 combined-batch1 第七轮 69/69 completed——含三次人工救场（user_paused 让行、Zotero-Word 集成模态、重开 STALE_REF）与一次 COM 线程修复，非无干预跑通；单链 VS Code 26/26、Zotero 9/9、Excel 13/13。timing sweep 整轮当前受阻于显示端表面被盖（covered_or_offscreen），预检阻塞路径已真机验证。无干预整跑、误触自动恢复真机验收与 60 秒观察阻塞定位仍待，见 [ROADMAP](docs/ROADMAP.md)。

## [0.1.6] - 2026-09-28

本版新增可恢复的 YAML 流程、任务历史与桌面人工交接，强化观察完整性、输入目标核验和窗口归属，并整理桌面提示与控制界面。

### Changed

- **[BC] UIA 观察结果更严格**：`get` 保留空字符串并增加 `unsupported`，仅在 Pattern 明确不支持时回退；`snapshot` 标明节点/深度截断，属性和子树读取失败不再伪装为空值或空树。`find`、旧定位/ref 重放和表格提取拒绝不完整观察，元素等待仅把 `NOT_FOUND` 当作缺席；表格仅在明确不支持 GridPattern 时回退。[观察说明](docs/37-uia-observation.md)
- **[BC] 桌面交接与显示协议 v6**：副作用动作等待匹配显示端就绪；桌面授权须由用户点击开始/继续或按 `Ctrl+Alt+Y`，外部键鼠输入撤销授权并请求让行。暂停和拖动状态条不写粘性急停，关闭显示端撤销授权并使租约失效；显式急停保留来源、时间和进程信息，重启不清除，解除后任务仍需另行继续。请配套更新 `actl` 与 `actl-signal`。[交接说明](docs/32-desktop-handoff.md) · [急停生命周期](docs/35-stop-lifecycle.md)
- **桌面界面与文案**：交接卡和任务详情使用深色圆角外框、分级字号与原生可访问控件；默认展示任务、状态、目标或进度和下一步提示，编号及原始诊断收进可复制、可滚动的详细信息。暂停/继续按状态显示，急停按钮与快捷键同行；托盘重复入口合并为“任务详情”，退出项改为“关闭”。长标题省略、布局随 DPI 调整，支持悬停、按下、禁用与键盘焦点反馈。
- **显示绘制**：状态条为 280×34 DIP，状态点 12 DIP，使用 Direct2D/DirectWrite 和逐像素透明圆角。屏幕边缘按目标显示器及 DPI 呈现运行、等待和暂停状态；减少动态设置下静止，截图时隐藏，不拦截点击。拉丁和数字使用内嵌 Source Code Pro Medium，中文优先使用思源黑体并提供系统字体回退。[显示实现](docs/18-display-layer-implementation.md)
- **构建与维护**：YAML 解析器及标准文件锁所需的声明 MSRV 提高至 1.89；实际工具链仍由 `rust-toolchain.toml` 锁定。协议默认超时集中到 `actl-core::timing`，内部轮询保留环境变量调优，命令默认值不变。显示端按职责拆分，gate/regress 在 `target/validation/release` 构建配套程序。

### Added


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。

- **popup_ignores 弹层门禁降噪第二轮（spec，[06 运行时治理](docs/06-design-blueprint.md)——两个独立应用同根因升级为类问题后的声明式收口）**：门禁观察面分不清「常驻部件」与「真弹窗」——Word 停靠导航窗格（fill-3 误拦）+ VS Code Find/Replace 面板（常驻 UIA 表面，关闭仅视觉折叠、带按钮挡一切写步=结构性死锁）两实例实证。**词表启发退役**（is_docked_pane 面板词白名单不泛化，第二实例即滑过）；**provider 可见性路线探针否证**（VS Code 折叠面板仍报 offscreen=false——Electron 对视觉折叠不说实话，如实入档）。正解=声明认领：popup_ignores: [{app, name_contains}]，门禁三处同口径跳过（步前判定/pending 过滤/处置后复查）；name_contains 必填（最窄形态，禁止掏空门禁的宽声明）；同表面 handler+ignore 双声明 PROTOCOL（处置与放行互斥）；与 handler 双层共存合法。Word 模板按 010 实证补认领（.docx+导航）。测试：popup_gate +4（认领放行/词不符仍拦/双层共存处置+认领两不相扰/schema 互斥拒绝）；uia 词表测试随退役删除。真机验收通过（2026-10-11，VS Code 主轮 v4 修订同现场 A/B）：v3 同现场被常驻面板死锁挡住的 find-replace/quick-open×2，认领声明后全部 completed+verified——同一现场的对照铁证；run 收于 25/26（close 步 precondition 阻断于操作者中途清窗导致的 bind 陈旧，非机制缺陷，手动等价收尾+进程零残留）。
- **YAML 流程 v1**：新增 `actl-flow` 与 `flow-validate/run/status/pause/resume`，提供 Schema、字段级错误、全步骤预检、参数数组、不可覆盖计划、写前记录、命令边界暂停和恢复前验证。写操作不自动重试，结果不确定时先核对；托盘支持当前流程暂停/继续。[流程协议](docs/20-yaml-workflow-protocol.md)
- **流程数据与恢复**：步骤输出可通过 JSON Pointer 导出显式类型的数据，并通过 `bind` 绑定后续参数；`max_age_ms` 拒绝过期或时间未知的数据。新增 `retry: stale_read` 和 `resume_check`，支持只读陈旧引用的有界重新定位与恢复上下文检查；写入后的观察失败即使指定 `--retry-step` 也不重放写步骤。[输出绑定](docs/38-flow-output-bindings.md) · [恢复说明](docs/39-flow-observation-recovery.md)
- **任务历史与诊断**：新增 `history` 和 `task-archive`，通过 `ACTL_TASK_ID` 及 run/step/call/action 关联跨调用事件、报告和证据，区分执行事实、推测与用户反馈。记录日志健康、完整性和控制请求的 `request_id`，区分调用完成与流程完成，保留安全的送达元数据；默认不记录正文、标题或剪贴板，不清理桌面恢复记录。[日志元数据](docs/41-logging-metadata.md)
- **UIA 组合定位**：支持 `uia:<JSON>`，将角色、AutomationId、精确名称或名称包含作 AND 匹配，并限定唯一容器的子级/后代；歧义、观察失败和遍历超限明确拒绝，不静默降级。`find --target` 提供同语义诊断，流程在执行前校验目标结构。[定位说明](docs/33-uia-composite-selectors.md)
- **输入与结果断言**：`press --target <selector> [--snapshot <id>]` 核验现有输入焦点；`type --verify-value <expected>` 在同一控件读回完整值，失败保留送达信息。click/press 支持 `--expect element:<target>`，有界等待目标出现，失败返回 `ASSERTION_FAILED` 并附原因。流程中的 verify 区分 Name/Value/Text，wait 支持 Text。
- **空闲等待与显式物理点击**：新增 `wait --idle <ms>`，等待系统键鼠空闲；指针点击族与 hover 支持 `--no-activate`，须显式 `--physical`，结束后恢复窗口标志和光标，异常残留按进程实例核对恢复。drag 和键盘命令不支持该选项，仍受目标、焦点和桌面交接护栏约束。
- **窗口与显示管理**：新增 `list-windows --native` 及 `--native --hwnd`，可观察隐藏/无标题窗口与指定子句柄；新增 `display-start/status`、`minimize/restore-window`、`desktop-prepare/restore`。窗口整理保存恢复记录并报告跳过项，`launch --wait-window` 提供就绪等待。
- **场景与回归入口**：regress 支持清单、suite/case 筛选、二进制哈希与分离日志。增加 Office 编辑保存重开、VS Code 文件编辑、Zotero 搜索、夸克与浏览器隔离场景；统一任务标识、原生窗口归属核验、人工让行保留现场及离线清理测试。应用知识归入 pack，业务流程统一放在 `skills/actl/workflows/`。[场景记录](docs/25-deep-edit-search-tests.md)

### Fixed

- **尾段挂起族根杀（fix，[70 §4](docs/70-debug-playbook.md)——06 定位/06 主修复/08 新签名暴露的类级终局）**：13112 签名实证 process::exit 走 CRT 清理可能到不了 OS（flow-resume envelope 落盘后 CPU 恒零赖活数小时）——析构链上的 UIElement 跨进程 Release/thread_local 定位缓存/signal worker join 全是无界阻塞面。inish 两分支改 envelope flush 后 hard_exit_after_envelope（watchdog 新增：直接 TerminateProcess，绕过一切运行时清理——envelope 已 flush、状态已显式落盘，不依赖析构面）；SignalSession::stop_worker 改 detach（join 曾把 finish 卡在 envelope 打印之前=「空输出」形态）。真机实证：list-windows 403 窗僵尸进程清零。配套 docs/70 §4 两条纪律（envelope 先落盘再解析/二次楔死即换通道）。
- **交接与控制反馈**：授权后的 750ms 宽限窗吸收本次点击/按键余韵，之后恢复严格接管检测；快捷键不可用时提示点击按钮。控制窗口固定任务绑定，暂停/继续在后台执行并保留失败反馈；继续入口检查急停、可恢复状态及 800ms 暂停稳定时间，区分“已请求暂停”和“已暂停”，状态读取失败明确显示未知。
- **目标身份与输入焦点**：带 `--snapshot` 的 ref 在实际命中时核验窗口与控件身份，缺失 RuntimeId 不放行；输入前及注入边界重验具体焦点。HWND 键盘绑定按句柄判断，不再把句柄误当标题匹配；`focus-window` 核验原生前台，只读观察不隐式激活窗口。
- **执行与暂停**：流程预检提前拒绝无条件 wait/verify、错误选择器与等待谓词，避免前序动作生效后才发现配置错误。流程 wait 在轮询时响应暂停/停止，`wait --window` 使用原生窗口轮询，减少 UIA 阻塞；输入锁观测正确释放所获锁，等待后复查停止，key-up 与修饰点击如实记录影响。
- **窗口观察与清理**：batch 使用原生 HWND 基线，枚举失败保留已执行步骤并报告 `window_observation_error`，不推断无新增窗口。fixture 使用独立文件/profile、HWND/PID/进程实例与任务归属核验；不关闭归属不明的窗口或用户恢复的标签。CLI 退出后读取日志有界，避免 GUI 继承管道导致无限等待。

### Documentation and validation

- 增加 winapp 只读对照诊断、浏览器连接原型、自动测试评估和探索模式设计记录。浏览器原型独立于正式 CLI/YAML；探索模式、MCP 适配和游戏方向尚未实现。MCP 规划为在被控 Windows 设备本地启动，Linux/macOS 执行后端不在当前范围。[浏览器原型](docs/34-browser-connection-prototype.md) · [winapp 对照](docs/36-winapp-uia-diagnostics.md) · [探索提案](docs/42-exploration-mode.md)
- 代码门禁与真实桌面验收分别记录。部分系统 fixture、浏览器原型、最新界面交互、跨实体显示器和屏幕阅读器验收仍待完成；局部通过不代表全量真机回归通过，具体证据和限制见对应文档及 [ROADMAP](docs/ROADMAP.md)。

## [0.1.5] - 2026-09-27

浏览器真实会话的内容通道、视觉兜底闭环、输入与寻址面规范化。场景实测与
踩坑细节见 docs/16 与 git 日志,此处只记变更本体。

### Added


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。
- **浏览器内容通道**:`snapshot --view content` 内容域投影(应用工具栏等
  chrome 整枝丢弃,与 `--skeleton` 组合对浏览器页面的 token 削减达 70-80%);
  `wait --stable` settle 谓词(观测样本连续两次不变即就绪,滚动/导航后的
  加载判定,替代固定 sleep)。
- **坐标寻址 `xy:<x>,<y>`(视觉兜底的"手")**:click 家族与 hover 接受屏幕
  坐标,面向 UIA 失明区(自绘菜单、无名按钮、Canvas)——screenshot 出图、
  多模态读坐标、`click xy:` 落点,闭环补全。守卫:强制 `--app` + 显式
  `--physical` + 窗口边界检查(坐标须落在目标窗口当前 bounds 内,窗口
  移动即拒)。
- **寻址与窗口锚补强**:`id:<Id>#N` 序数(多文档页同名 AutomationId);
  `--app hwnd:<n>` 句柄锚(SPA 站点导航改标题后锚点不失效);
  `snapshot --hwnd`(任务栏 XAML 岛等无标题特殊窗可达)。
- **输入面补全**:middle-click(新标签页/新实例)、triple-click(段落选)、
  `--with ctrl,shift`(修饰键与点击原子组合,多选/范围选);`press win`/
  `press shift` 等裸修饰键可用。
- **浏览器应用档案 pack**(quirks + 路线,Edge/Chrome 双实测背书)与
  SKILL 的 actl×playwright 分工协议;pack 契约校验入 gate。
- **文档**:docs/15 系统 UI 区域普查、docs/16 浏览器场景战记。

### Fixed
- **wait 族对 AMBIGUOUS 快速失败**:确定性多命中原先会等满超时并报误导性
  TIMEOUT,现在立即失败并附候选。
- **[BC] `list-windows` 的 rect 统一为物理像素**:此前为逻辑坐标,与截图/
  坐标寻址在高 DPI 多屏下跨缩放混层;依赖旧值的消费方需按 DPI 换算迁移。

## [0.1.4] - 2026-09-27

M2 收官批:命令面 27/31 + 窗口治理与可观测性 + 信号条重设计 + Office/微信场景实测
+ 应用档案 pack 体系。

### Added


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。
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


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。
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


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。
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


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。
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


- **popup_ignores pack 级来源（spec，[06 运行时治理](docs/06-design-blueprint.md)——逐模板抄窗格清单不可扩展的收口,release-review-08 回归根治）**：apps/*.yml 顶层 popup_ignores: [{app, name_contains}]（与 plan 级同构,pack 列全锚形态——Word=Word/.docx 两种）,CLI load_plan 自动并入模板级声明（并集去重/幂等/stderr 留痕）;注入用行级状态机提取（pack 是松散人读文档,严格 YAML 解析对 quirks 冒号形态炸——word/cloudmusic 实证）;lint-packs ⑥ 号检查守门（非空 app+name_contains）。**四个模板的手写声明全部撤除（pack 单一来源）**——新 Word 模板零声明成本即得导航/搜索降噪。测试：extract 纯函数 2（引号对/空值跳过/块边界）+E2E 契约 1（emit-plan 断言注入条目,仓库根 cwd）。

- **browser-read --wait-for N 沉降谓词（spec，[06 浏览器节](docs/06-design-blueprint.md)——CDP 轨缺沉降原语补面,43 联测暴露）**：命中数 ≥ N 才返回,内建 500ms 轮询共用药洞超时,超时 TIMEOUT 附实测命中数（异步渲染差多少一目了然）；与 --all/唯一读通用。背景:browser 轨无 wait 谓词（wait 是 UIA 轨）,异步渲染页（GitHub hpc 列表 CSR ~3s）读到半成品,04/06 轮 output field missing 根因;外挂 ps1 轮询循环（用户两度指正:为何不用 yml/actl——基建缺口实为根因,修基建让场景塌缩,探测降为单命令）。真机实证:zotero_cli releases 页 --wait-for 2 等 3s 沉降后返回 19 元素。测试:契约 help 四要素。

- **popup_ignores 弹层门禁降噪第二轮（spec，[06 运行时治理](docs/06-design-blueprint.md)——两个独立应用同根因升级为类问题后的声明式收口）**：门禁观察面分不清「常驻部件」与「真弹窗」——Word 停靠导航窗格（fill-3 误拦）+ VS Code Find/Replace 面板（常驻 UIA 表面，关闭仅视觉折叠、带按钮挡一切写步=结构性死锁）两实例实证。**词表启发退役**（is_docked_pane 面板词白名单不泛化，第二实例即滑过）；**provider 可见性路线探针否证**（VS Code 折叠面板仍报 offscreen=false——Electron 对视觉折叠不说实话，如实入档）。正解=声明认领：popup_ignores: [{app, name_contains}]，门禁三处同口径跳过（步前判定/pending 过滤/处置后复查）；name_contains 必填（最窄形态，禁止掏空门禁的宽声明）；同表面 handler+ignore 双声明 PROTOCOL（处置与放行互斥）；与 handler 双层共存合法。Word 模板按 010 实证补认领（.docx+导航）。测试：popup_gate +4（认领放行/词不符仍拦/双层共存处置+认领两不相扰/schema 互斥拒绝）；uia 词表测试随退役删除。真机验收通过（2026-10-11，VS Code 主轮 v4 修订同现场 A/B）：v3 同现场被常驻面板死锁挡住的 find-replace/quick-open×2，认领声明后全部 completed+verified——同一现场的对照铁证；run 收于 25/26（close 步 precondition 阻断于操作者中途清窗导致的 bind 陈旧，非机制缺陷，手动等价收尾+进程零残留）。
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
