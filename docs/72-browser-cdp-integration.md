# 72 · 浏览器 CDP 正式接入设计(A1 自研最小客户端)与实现验收

日期:2026-10-02(设计+spike)/2026-10-02(同日 v1 实现落地,用户授权"继续实现")。
用户授权:路线 A(自研同步最小 CDP 客户端)+ 系统 Edge 二进制 + actl 专用持久 profile;协议面 spec 同 PR 落地。
关联:[34 浏览器原型](34-browser-connection-prototype.md) · [16 浏览器场景](16-browser-scenarios.md) · [06 §3 浏览器会话命令族](06-design-blueprint.md) · [PRD §1.3](PRD.md) · [47 执行恢复](47-execution-recovery.md) · [60 流程资源](60-workflow-resources.md) · 原型 [prototypes/browser-rs](../prototypes/browser-rs/) · 实现 `crates/actl-browser`

## 1. 决策摘要

| 决策点 | 结论 | 依据 |
|---|---|---|
| 传输载体 | **A1 自研同步最小 CDP 客户端**(tungstenite + 无类型 JSON) | §2.3/§2.4 体积与漂移实证 |
| 被驱浏览器 | 系统 Edge 二进制(`msedge.exe`),不下载、不打包 | §2.2;fixture 纪律(系统自带应用) |
| 窗口形态 | **默认有头**;`--headless` 为后台选项 | 用户裁定(2026-10-02):可见=非隐身执行立场+常规 UA 过壁垒+人工登录就绪+UIA 兜底面在场;headless 的 UA 拒服务见 [73](73-browser-twitter-scenario.md) |
| 会话来源 | 一次性隔离 profile / **持久专用 profile**(一次性人工登录复用)双模式 | §4 |
| 通道分工 | 与 UIA 按"会话归属"分工:用户浏览器→UIA;actl 自有浏览器→CDP | §3 |
| 依赖计划 | 正式实现仅新增 tungstenite(直接依赖 7→8,≤9 预算内),TECH_STACK §3 登记 | §8 |

## 2. 调研事实(2026-10-02 核实)

### 2.1 默认 profile 禁 CDP(硬约束)

Chrome/Edge 136 起 `--remote-debugging-port/pipe` 仅在**非默认** `--user-data-dir` 下生效,默认 profile 静默不开端口。上游:Chrome 官方公告(developer.chrome.com/blog/remote-debugging-port)与 Microsoft Learn Q&A(Edge 报 "DevTools remote debugging requires a non-default data directory")。失败形态阴险:端口"看似连接后挂死"(cc-skills #151)——**健康检查不能只看端口可达**。本机 2026-10-02 活体验证见 §10 run-1。

### 2.2 生态对比

| 方案 | 判定 |
|---|---|
| chromiumoxide 0.9.1 | 异步 tokio,违反 ADR-001,出局 |
| headless_chrome 1.0.22(3.0k star,同步) | 可用但高层 API 整体链入 auto_generate_cdp 全量类型化协议,§2.3 体积破线 |
| Playwright 旁挂进程(Python/Node) | 违反单二进制零运行时依赖 NFR,降级为过渡选项 |
| WebView2 内嵌(CallDevToolsProtocolMethod) | 零新增依赖、复用显示层经验;无官方 headless;列为可见会话形态的备选,M3 WebView2 专项验收后评估 |

### 2.3 体积实测(本机同日头内,actl 同款发布配置 lto+strip;量级可依赖,绝对值不跨环境比较)

| 探针 | 体积 |
|---|---|
| 空程序基线 | 0.11 MB |
| + headless_chrome(全 API 面) | 8.85 MB(增量 ~8.7 MB) |
| + tungstenite(手写全部外部依赖) | 0.32 MB |
| **A1 spike 完整探针**(tungstenite+serde+sha2+全部探针代码) | **0.77 MB** |

对照现状:actl.exe 5.22 MB、actl-signal.exe 1.17 MB。A1 并入后主二进制约 5.5–6 MB,单二进制 <10MB NFR 保持;headless_chrome 并入则 ~14 MB,破线。

### 2.4 版本漂移

A1 只用无类型 JSON + 稳定域(Target/Runtime/Page/Browser),协议面为这几个域的历史稳定方法;不随浏览器版本重编类型(chromiumoxide 0.6→0.9 为 Chrome 142 线兼容跳版即全量类型化绑定的反面教材)。

## 3. 架构定位:第三个后端

```
actl CLI / flow(统一 envelope、错误码、人工交接、急停)
  ├─ actl-uia      UIA 后端:用户日常浏览器 + 浏览器 chrome 层 UI + 全部桌面应用
  ├─ actl-physical 物理兜底(显式 --physical)
  └─ actl-browser  CDP 后端(本设计):actl 自有浏览器会话
```

- 通道按会话归属选择,flow 步骤级声明;同一 flow 可混用(UIA→CDP→UIA)。
- actl-browser 遵守既有硬规则:无 async、unsafe 禁入(actl-uia 之外)、stdout 仅 envelope。
- 浏览器 chrome 层(工具栏/原生对话框)仍归 UIA;CDP 只覆盖页面域。

## 4. 会话来源(正交维度)

| 模式 | profile | 登录态 | 用途 |
|---|---|---|---|
| ephemeral | 一次性临时目录 | 无 | 匿名任务 |
| **persistent** | 命名持久目录,actl 拥有 | 一次性人工登录后跨任务复用 | 需登录的普通任务 |

- persistent 的一次性登录是协作式人工交接暂停点(扫码/输密码由用户完成),登录动作不自动化、不代填。
- profile 目录登记为受保护流程资源(docs/60 契约),路径归 actl 数据目录管辖。
- **cookie 搬运/复制用户 profile 明确排除**(沿 docs/16 §5.5 安全边界)。

## 5. 连接层设计(spike 已验证形态)

- **单属主事件循环 + mpsc**:socket 仅被循环线程持有(非阻塞读 + 出站队列 + WouldBlock 重发箱);调用方投递请求并限时阻塞收响应;事件全部外抛。无 async、无共享 socket 锁竞争。
- 自建 TcpStream 握手后切非阻塞(tungstenite `connect()` 的 MaybeTlsStream 不暴露该控制)。
- 错误码:连接层 `DISCONNECTED/TIMEOUT/SEND/CONNECT`,协议层 `CDP_<code>`(带 message+data)。
- HTTP 发现端点(`/json/version`)**必须按头解析、按 Content-Length 读体**:Chromium DevTools HTTP 不保证按 `Connection: close` 断开,`read_to_end` 会吃读超时(§10 run-2 实证,Windows 上读超时同样报 10060,易误诊为连接被拒)。
- DevToolsActivePort 用 `--remote-debugging-port=0` 自动分配,从 profile 目录读取。

## 6. 身份绑定与导航竞态

- 绑定元组:browser endpoint + **CDP targetId**(精确,非 URL 索引)+ 导航后 frameId + 任务标记。同 URL 干扰 target 不能替代(spike 验证)。
- **写入时重核**:每次写操作前重验 session 对应 target 仍存在且 URL 未漂移;导航竞态检测前提是事件流可达——`Page.frameNavigated` 经读线程接收已实证(10ms 级)。
- 竞态处理语义(协议面定稿时入 spec):绑定失效→专用错误码(参照 STALE_REF 的恢复指引形态:重绑→重核→重放仅限未派发的写)。

## 7. uncertain 与持久化恢复

- **uncertain 治理归 flow 引擎 WAL(现行态,对齐 docs/47 execution-recovery)**:写步(含 browser-click/type,同一 dispatch 链)派发前置 Running 态落盘——持久化失败阻止副作用(engine.rs「Write-ahead record is required」);进程崩溃后 resume 对 Running/Uncertain 步**先只读 recheck verify**——通过转 Completed 不重放,不通过转 NeedsReview 等显式 `--retry-step`;recheck 确认前禁止重放。
- 早期 A1 spike 的进程内 `write_state` 内存态已随架构定稿移除(单 CLI 进程一命令到底,进程退出内存态即消失,无跨命令内存 uncertain 场景)——曾登记于此的「已实现(spike)」描述过时删除。

## 8. 安全护栏

1. **cookie 不可读出**:浅层手写客户端按域白名单实现(Target/Runtime/Page/Browser),cookie 读取域不在其实现面内(构造性不可达)。深水层(chromiumoxide,2026-10-13)类型面覆盖全协议域、Network/Storage 类型**可达**——安全誓言转移到**命令面**:CLI 永不提供 cookie 导出/转储动词,由负面契约测试 `browser_command_surface_has_no_cookie_verbs` 钉死;cookie 读取域永久列入负面清单。
2. **port vs pipe**:`--remote-debugging-port` 是本地无鉴权端口,同机任意进程可连(等于可读该 profile 全部凭据)。pipe(`--remote-debugging-pipe`)为 Chromium 的 POSIX-only 实现,**Windows 上进程即退(exit 13),已判死**——风险缓解为 launch 层硬编码 127.0.0.1 回环绑定 + 短生命周期会话。
3. 不附着用户已有浏览器会话;仅操作 actl 启动的实例(与原型边界一致)。
4. 急停/人工交接:CDP 写操作接入既有停止代次与显示协议,语义与物理输入一致(正式实现项)。
5. **深水层 attach 边界**:bridge 仅连接 actl 自有会话的端点(会话文件导出),不提供任意 URL attach;错误面沿既有映射(端点死=NOT_FOUND)。

## 9. 协议面草案(spec PR 时定稿,本节仅为范围)

- 命令族:browser 会话生命周期(connect/attach/close)、定位(CSS/XPath→JS 求值)、读写、写后核验;envelope 与错误码走 actl-core 统一映射。
- YAML:flow 步骤级通道声明 + browser 会话作为资源声明(ephemeral/persistent)。
- 硬规则 8:golden 契约测试 + docs/06 + CHANGELOG 同 PR。

## 10. spike 真机证据(2026-10-02,Windows,msedge Edg/154.0.4258.48,sha256 64670d59…)

环境:开发机,无急停;探针源码与报告落 `target/browser-rs-probe-20261002-{1..5}`;离线单测 6/6(路由/错误 golden/写入闸门状态机)。

| 轮次 | 结果 | 证据要点 |
|---|---|---|
| run-1 | 失败 | **相对 user-data-dir 被 Edge 静默弃用→落入默认目录→136 规则禁端口**(§2.1 活体);实例无端口无 HWND,优雅关闭不可达(见 §11 偏差记录) |
| run-2 | 失败 | DevToolsActivePort 已写出(绝对路径修复生效);HTTP 发现端点读超时(§5 教训);该实例经 CDP Browser.close 优雅关闭成功 |
| run-3 | 核心 8 项 PASS | OOPIF 探索失败:getTargets 不暴露 iframe target(headless 未启用站点隔离) |
| run-4 | 同上 | frame tree 在场且跨源确认;本探针 JSON 指针少一层(自身 bug) |
| **run-5** | **9/9 PASS,exit 0** | **全链**:启动→双 target 绑定→Page.enable→导航+`frameNavigated` 事件(读线程)→CSS 点击写后核验→XPath 读回→**跨源 iframe 经执行上下文(contextId)点击与核验**→同 URL 干扰 target 隔离→断连显式 `DISCONNECTED`→重连只读 recheck(counter=1 未重放)→CDP `Browser.close` 优雅退出(exit 0)。全程 wall 3215ms |

跨源 iframe 结论:headless Edge 站点隔离关闭时 iframe 无独立 target,`Page.getFrameTree` 定位子 frame + `Runtime.executionContextCreated` 事件配对 contextId + 定向 evaluate 是可行且已验证的路线(优于 OOPIF target 路线,两种隔离配置下均工作)。

## 11. 设计教训与偏差记录

1. **launcher 不变量:启动前 user-data-dir 必须绝对化**。违反后果链:默认目录→136 禁端口→Browser.close 永久不可达→清理只剩强杀(run-1)。正式实现的 launch 代码把"绝对路径+端口文件出现"作为启动成功判定,失败即 fail-fast。
2. **优雅关闭可达性是启动承诺的一部分**:actl-browser 必须保证自己启动的实例总能经 CDP 关闭;debug port 被禁用视为启动失败而非降级运行。
3. 偏差如实记录:run-1 遗留实例在 close-window 被显示端版本失配拒绝、两次 WM_CLOSE 被无头窗口忽略后,经归属核实(命令行/启动时间/空 profile/about:blank/无会话状态可复活)以 Stop-Process 终止——与 AGENTS taskkill 禁令的字面冲突在此记录;该类实例的"会话复活"风险不存在,但正式实现以 §11.1/§11.2 不变量杜绝再次出现。

## 12. v1 实现(2026-10-02 spec 同日落地)

按 §9 范围实现完整命令族:`crates/actl-browser`(连接层=cdp.rs 单属主事件循环移植、
session.rs 会话文件原子落 `%LOCALAPPDATA%\actl\browser\`、ops.rs 五操作)+ actl-cli
`browser.rs` + help 四要素 + 契约 golden 8 项(离线,CI 安全)。tungstenite 登记
TECH_STACK §3(直接依赖 8/9)。错误码复用全集:CONNECT→NOT_FOUND(端点死=会话失效)、
TIMEOUT→Timeout、其余→Internal;无新增码。

**真机验收(14 步全绿)**:launch(Edg/154.0.4258.48)→read "0"→守卫写+expect-text
"clicked-1"→效果读 "1"→XPath 读→AMBIGUOUS(3 matches)→NOT_FOUND→iframe 读"0"→
iframe 守卫写+核验→iframe 读"1"→navigate(标题+计数归零证明新鲜加载)→close
(closed:true+ephemeral profile 删除)→二次 close 幂等 NOT_FOUND。证据:
`target/browser-cli-accept/acceptance.jsonl`、管道行为 `pipe-test.ps1` 复验。

**新增设计教训(v1 实现期实证)**:

4. **stdio 继承泄漏是分离式子进程的产品级 bug**:浏览器进程树继承调用方管道写端,
   actl 退出后 `$()`/`ReadToEnd`(原生 Windows 管道实测复现)永不 EOF——所有管道捕获型
   agent 调用方都会挂死;`Stdio::null()` 三件套(interact.rs launch 对 Word 的既有修法)
   对 Edge 多子进程树不够。修复=派生前清自身三件 stdio 的 INHERIT 标志
   (`actl-uia::process_io::clear_stdio_inheritance`,Win32 直调集中于 actl-uia)。
   此守卫应作为今后一切长命子进程派生的固定前置。
5. `--expect-text` 契约澄清:断言**被点击元素自身**的写后文本(写后核验的"验"落在
   被驱动对象上);效果断言(计数变化等)归 browser-read,两层分开。

## 13. 开放项

- [x] flow/YAML 通道接入(2026-10-02 实测通过):browser 命令可入 YAML 流程,治理三件套
  各就各位——① outputs 只允许只读生产者→补 `browser-list`(只读枚举,新→旧)承载会话句柄
  产出(pointer /sessions/0/session_id),`browser-launch` 保持生命周期命令不带 outputs;
  ② 副作用步骤必须带 verify→补 `browser-verify` 断言族(--newest-alive/--none/
  --session+--alive|--gone,全只读);③ `read_only` 分类纳入 BrowserList/BrowserRead/
  BrowserVerify。bind 注入 `--session` 值位(下标须指值字面量)。探针流程
  `target/browser-flow-test/browser-flow-probe.yml` 六步真机全通(completed+verified,
  会话句柄贯穿,收尾 browser-list 归零、零残留进程)。**治理发现**:未声明 handoff 的
  流程里浏览器写不触发审批门(CDP 写非桌面物理输入,引擎按命令分类裁决)——授权语义
  =停止代次核验(browser-click 已过);动作卡/审批卡与显示端的接线归显示层待办(§13)。
  (写入 uncertain 跨进程持久化已由引擎 WAL 覆盖——见 §7 现行态,余量行删除。)
- [x] **持久 profile 四件套(2026-10-02 实测通过)**:① 登录暂停点——`pause_before: true` +
  `resume_check: browser-verify --session X --css <登录标记>`(本地页全链:暂停→恢复通过→
  完成;谓词不满足→`reason: resume_check_failed` 保持 paused);② 登录态核验谓词——
  browser-verify 选择器模式(--session+--css/--xpath [--contains],唯一解析+文本包含,
  不满足 ASSERTION_FAILED);③ profile 资产治理——flow 资源新种类 `browser_profiles`
  (docs/60:全局命名资产、创建幂等、永不删除;流程内 --profile 未声明即 PROTOCOL 拒);
  ④ 并发治理——同名持久 profile 再启=复用活跃会话(reused:true),端点已死的陈旧记录自动
  接管。示例流程 `skills/actl/workflows/browser-login-session.yml`。**真实站点登录验收仍待
  用户选站**(本地页已验机制,登录动作本身归用户)。新增教训:**持久 profile 目录会残留
  上次退出的 DevToolsActivePort**——启动成功判定必须"文件出现且端点应答 /json/version"
  双条件,且 spawn 前清除陈旧文件(ephemeral 每次新目录故从未暴露)。扩展策略:ephemeral
  恒 --disable-extensions;持久 profile 默认保留扩展(合法自动化资产,如猫抓),
  --disable-extensions 显式禁。用户日常浏览器与 CDP 会话是两个数据世界,互不影响。
- [x] **读写补全与显示协同(P1+P2,2026-10-02 实测通过)**:`browser-type`(守卫输入写:
  input/textarea 唯一匹配→native setter+input/change 事件→读回值核验,内容不回显只报长度,
  非 input/受控组件改写分别 NOT_ACTIONABLE/ASSERTION_FAILED)与 `browser-read --property`
  (白名单 value/href/name/id/checked/tag——x.com 场景的 input 值缺口闭合);P2=写与生命周期
  命令声明信号动作卡(launch/navigate/click/type/close,kind+cdp 会话标记 target,
  InputEffects 全空,无租约门——展示透明性而非权限门;动作日志验证 preparing/executing/
  delivered/failed 全周期在场)。真机 13 步验收 `target/browser-p1p2-accept/`。
- [ ] 显示端会话卡(常驻浏览器会话 UI,非动作流);close 后 ephemeral profile 延迟重删的残余竞争(7 轮 2 次失败后已收口,复发再议)
- [ ] **深水层终态吸收**:现有动词迁移到 chromiumoxide 骨干、退役手写传输层(cdp.rs)与 tungstenite 依赖——双栈过渡态是显式接受的中间态,吸收时机由 ROADMAP 裁定;进一步深水层动词(协议级输入/下载管理/Fetch 拦截改写)按真实场景逐个 spec
- [ ] WebView2 内嵌形态评估(并入 M3 WebView2 专项验收后)
