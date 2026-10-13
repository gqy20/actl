# 浏览器库引入 spike 决策简报(2026-10-13)

目的:为「actl-browser 原地吸收一个浏览器库」提供证据。双候选(chromiumoxide 0.9.1 / headless_chrome 1.0.22)同日同机同场景对照。工作区:worktree `E:/explore/computer_cli-spike`(branch `spike/browser-lib-probe`),探针源码 `prototypes/browser-lib-probe/`,本目录即证据。

## 一、对照矩阵(全部真机实测,Win11 25H2,msedge)

| 维度 | chromiumoxide 0.9.1 | headless_chrome 1.0.22 |
|---|---|---|
| attach 既有 ws 端点 | ✅ `Browser::connect`,30-42ms | ✅ `Browser::connect`,133-158ms(docs 面未披露,源码+实测证实) |
| attach 后 drop 误杀 actl 会话 | ✅ 无(存活验证过) | ✅ 无(connect 路径 close_on_drop=false,实证) |
| 深能力:Fetch 域请求拦截 | ✅ intercepted=2(裸)/8(带扩展) | ✅ intercepted=2/8——**须显式 `enable_fetch(None,None)`,`enable_request_interception` 单独调用是静默 no-op** |
| 同步门面桥(ADR-001 关切) | ✅ sync main + 受控 runtime + block_on 全链真机工作;handler 泵任务由桥持有 | 不需要(原生同步) |
| 自启(launch 不变量) | ✅ 绝对 user-data-dir、new headless、872ms、`Browser.close` 1ms 显式优雅关闭、零残留 | ✅ 同上(887ms;drop→Browser.Close)、零残留、pid 退出实证 |
| stdio 继承泄漏(docs/72 §12 教训) | ✅ 无(带超时管道捕获作业正常完成) | ✅ 无 |
| persistent profile 扩展世界 | ✅ 同一端点;拦截看到扩展背景页流量(8>2) | ✅ 同上 |
| 体积(探针, lto+strip) | 6.82 MB(增量 ~6.7) | 8.66 MB(增量 ~8.55;复现 docs/72 §2.3 旧测) |
| 时序(attach 全程) | 1600-2000ms | 2385-2485ms |
| 集成怪癖 | handler 必须被泵送否则事件断流;桥接需防重入/死锁 | attach 默认 30s idle 断连(`connect_with_timeout` 可调);derive_builder setter 返回 `&mut Self` 链式陷阱;`wait_for_initial_tab` 已 deprecated |
| 错误面 | thiserror 类型化,映射 ErrorCode 结构好 | anyhow 内部,映射靠字符串判别 |
| 维护 | 4.9M 下载,2026-02 发版 | 3.4M 下载,2026-06 发版 |

基线 0.11 MB(与 docs/72 §2.3 旧基线一致,方法论可衔接)。actl.exe 现状 7.88 MB——**两库并入都会把单二进制推到 ~14-16 MB,<10MB NFR 两条路都破,量级相同,不构成区分项。**

## 二、spike 推翻的预判

1. 「headless_chrome launch 为中心、无 attach」——错,`connect` 是一等公民且实测安全;
2. 「chromiumoxide 的 async 是架构否决项」——错,同步门面桥把 async 圈在 crate 内部,外部 API 保持同步,真机全链通过。ADR-001 的修订(若选 co)收窄为一句:「actl-browser 允许在同步门面之后内部持有受控 runtime」。

即:**两库都是「原地吸收」的可行候选,没有出局者。**

## 三、建议

以用户既定方向(深度调用优先、原地迭代、可接受破坏性)为准:

- **倾向 chromiumoxide**:体积增量更小(6.7 vs 8.55MB)、全量类型化协议面(深度需求的长期资产)、错误结构化、维护更主流;代价 = 桥接层成为常驻组件(生产形态应为「专线程 runtime + 通道门面」,非裸 block_on),ADR-001 窄修订 + TECH_STACK §3 登记 tokio 族;
- **headless_chrome 为完全可行的替代**:零 async、线程+通道模型与现有 cdp.rs 同构(团队心智已在此模式上)、API(Tab/Element/wait_for_element)与动词形状更贴;代价 = 体积更大、idle 断连要常备覆盖、错误映射面弱。

## 四、集成阶段(无论选谁)的登记项

1. TECH_STACK §3 新增依赖登记(co: chromiumoxide+tokio 族 / hc: headless_chrome);
2. NFR 数字修订(单二进制 <10MB → 实测口径重定);
3. cookie 姿态重述:docs/72 §8「构造性不可达」→「类型可达、命令面不可达」+ CLI 负面 golden 钉死无 cookie 动词;
4. launch 不变量复验清单已由本 spike 预跑(绝对 user-data-dir/优雅关闭/stdio/零残留全绿),集成 PR 转为回归断言;
5. hc 特有:attach 一律 `connect_with_timeout`(idle 上调)+ 拦截走「interceptor+enable_fetch」双调用封装。

## 五、证据文件

- `round-attach.log` + `co-attach.txt`/`hc-attach.txt`(attach 轮,3 次迭代:缓存击穿+enable_fetch 修复后终局数据)
- `round-persistent.log` + `p-targets.json`(扩展 target 枚举:chrome-extension:// background 在场)+ `p-co-attach.txt`/`p-hc-attach.txt`
- `round-selflaunch.log` + `co-selflaunch.txt`/`hc-selflaunch.txt`(stdio 作业超时包裹)
- `launch.json`/`close.json`/`verify-alive.txt`(actl 生命周期留证)
- 探针源码:`prototypes/browser-lib-probe/{chromiumoxide-probe,headless-chrome-probe}/`
- 脚本:`run-attach-round.ps1`/`run-persistent-round.ps1`/`run-selflaunch-round.ps1`
- 许可清单:`license-inventory.txt`(三树逐包,Cargo.lock→registry manifest 实读)

## 六、许可证评估(2026-10-13 补轮,逐包实读非记忆)

方法:三树(co/hc/actl 基线)的 Cargo.lock 全量依赖闭包,逐包读 registry manifest 的 license 字段,落 `license-inventory.txt`。

### 结论一:chromiumoxide 全树干净

全树 MIT OR Apache-2.0/MIT/Apache 双许可(含已发布类型 crate `chromiumoxide_cdp 0.9.1` = MIT OR Apache-2.0——**协议类型在上游仓内预生成后作为普通 crate 发布,下游构建不跑生成器**);无 TLS 栈(本地 ws 不需要)、无 GPL、无特殊数据许可。选 MIT 即与本项目同构。cdp 类型源自 Chromium 协议定义(BSD-3 上游),生成物由 MIT/Apache 声明覆盖,常规 notices 清单吸收。

### 结论二:headless_chrome 存在 GPL-3.0 生成器风险(决策级)

事实链(全部实证):
1. hc 的协议类型模块由 **build-dependency `auto_generate_cdp 0.4.6` 在每次下游构建时生成**(hc `build.rs` 调 `init()`,产物写 OUT_DIR 编入二进制——我们的探针二进制里就有这些生成类型);
2. 该生成器是 **GPL-3.0**(LICENSE.txt 全文,无生成物豁免条款;作者 mdrokz,与 hc 项目不同源);
3. hc 自身 LICENSE.md 为 MIT(2022 contributors),**未对生成模块的许可冲突做任何声明**(README 亦无);
4. 无 feature 逃生口(default=["offline"] 恒链入生成器,关默认特性则协议模块不可用)。

两种读法:保守读法(生成物嵌入生成器模板的创造性表达,类比 bison 无豁免情形)→ 生成模块 GPL 传染 → 发布的二进制承担 GPL-3.0 义务(对应源码提供;静态链接的组合作品整体受 GPL 条款约束);宽松读法(对 Chromium BSD-3 协议 JSON 的机械变换,输出非衍生)→ MIT 成立。**未经法院检验,且上游无声明消解不确定性**。对发布二进制的 MIT 项目,该不确定本身即风险;actl 源码开源(MIT 与 GPL 兼容)可组合,但发布物有效许可姿态翻转为 GPL-bound,与仓库「不引入 vendored 外部代码」的卫生立场不符。

### 结论三:基线与余项

- actl 现状基线**本就携带纯 Apache-2.0 依赖**(uiautomation/uiautomation_derive/rmcp 族/sync_wrapper)——Apache-2.0 NOTICE 义务类别今天已存在,引入 co 只增加同类清单条目,不新增义务类别;
- hc 树附加 rustls TLS 栈许可面(ring Apache-2.0 AND ISC、rustls-webpki ISC、webpki-roots **CDLA-Permissive-2.0**——Mozilla 根证书数据许可;subtle BSD-3)——全部宽松许可,非阻断,但为 hc 体积与许可面的额外行李(本地 ws 用不上 TLS,却随默认树入链);
- tungstenite 版本对撞:co 树 0.28(async-tungstenite)/ hc 树 0.29 / actl 0.30——任何吸收路径过渡期都双栈共存,终态(吸收后退役手写传输)归一;
- 两树均无 GPL/AGPL/LGPL/MPL(除上述 hc 生成器问题),无传染性许可证阻断项。

### 许可面对建议的影响

「倾向 chromiumoxide」升级为「明确 chromiumoxide」:hc 的 GPL 生成器风险独立于所有技术维度,除非上游更换生成器、或项目方显式接受发布物 GPL 姿态,否则不应作为发布型二进制的依赖。此项不影响 hc 在纯内部/不发布场景的技术可用性。
