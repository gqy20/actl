# 单次观察长阻塞排查

## 结论与证据范围

2026-09-29，原始 `recovery-templates-final-20260929` 的最后一个 unless 检查耗时 60101ms 后返回 NOT_FOUND。该区间没有写入派发，也没有人工交接事件；不是 recovery 的等待间隔。旧日志没有内部阶段耗时或失败的 match/window 阶段，无法追溯证明某一个 COM 调用是根因。

本轮增加诊断后没有复现这次 60 秒故障，不能宣称已修复。证据在 `target/blocking-diagnose/`，旧任务 history 保存在 `target/recovery-live-20260929-173716/history-diagnose.json`。仅比较本轮同一桌面、同一 debug 构建的耗时。

## 实测

- 现有 Chrome 的 Document 查询先发现 6 个候选，按现场 RootWebArea 的 id/name 收紧后，8 组标题/HWND 值核验全部通过，共 16 次。标题路径 353–861ms，HWND 路径 121–150ms。原歧义 envelope 保留，没有将其记为通过。
- 用原冻结计划创建 `blocking-repro-20260929`，首次启动成功但切前台失败，保留 repro-run.json。诊断 CLI 位于 debug 目录，显示端位于安装目录；随后使用与显示端配套的已安装 CLI 切前台成功。不能据此把本次失败直接归因为旧 60 秒问题。
- 在上述未成功激活的窗口上，10 组输入框查询返回 NOT_FOUND；完整快照显示 RootWebArea 内容为空。切前台后恢复原流程，先核验不确定步骤，8 步最终全部 completed，恢复调用 7115ms。前台失败记录不覆盖。
- 页面就绪后，对同一个输入框再做 12 组标题/HWND 值核验，共 24 次全部通过。标题路径 346–1429ms，HWND 路径 90–145ms。其中 1429ms 调用有 1090ms 在 session.start；该次控件查询仅 38ms，读值 0ms。不能把命令总时长都算在 UIA 查询上。
- 上述 24 次的分层耗时：标题路径桌面枚举 236–417ms；两种路径的 FindAllBuildCache 为 24–46ms，ValuePattern 读取为 0–1ms。二进制路径及 SHA256 见 binaries.json。
- 标题路径正常时的额外耗时主要在桌面根枚举；HWND 路径绕开该枚举。它证明当前开销来源，不证明历史 60 秒也来自同一位置。已知窗口可绑定经归属核验的 HWND，并保留进程实例与目标身份校验，不能只凭可复用句柄长期认定身份。
- 测试窗口经程序/profile/PID/创建时间及唯一网页标签核验后正常关闭，verify-closed 确认原进程实例退出；用户原有窗口保留。

## 已补齐诊断

`ACTL_TRACE=1` 的 stderr 包含阶段 begin 与退出耗时。新增定位初始化、组合查询窗口解析、桌面枚举、HWND 转换、窗口候选属性、FindAllBuildCache、候选属性匹配、结果属性和 ValuePattern 读取。stdout envelope 不变；未启用时不输出。

复现时保存 stdout 与 stderr，最后一个未配对的 begin 可以定位仍在等待的调用层。trace 不记录输入正文或窗口标题。原有 scope 只有退出时打点，真正阻塞时看不到当前阶段，这一诊断缺口已补齐。

## 尚未证明与下一步

当前依赖的 UIAutomation::new 创建 CUIAutomation，代码没有设置 provider 事务时限。微软文档给出的 IUIAutomation2 默认连接超时为 2 秒、事务超时为 20 秒，但它不是整条命令的硬时限，不能凭 60 秒接近三倍就认定发生了三次超时。来源：[连接超时](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-put_connectiontimeout)、[事务超时](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-get_transactiontimeout)。

下一次慢调用应以新增阶段证据区分初始化、窗口发现、控件查询或读值，再决定是否限制 provider 调用或隔离只读执行。当前没有修改全局超时、切换 COM 客户端版本或把 provider 失败当作空结果；外层 recovery 仍不能强制中断同步调用。

本轮 `cargo xtask gate` 通过（gate.log），发布版 CLI 与显示端同步安装；安装版本哈希见 installed-binaries.json。真机批量数据来自 debug 构建，发布版仅另作只读观察和显示就绪核验，不混为同一轮性能样本。

## 2026-10-02：TIMEOUT 后续只读对照

这是独立于上述 60 秒 NOT_FOUND 的排障，不合并故障签名或跨日耗时。
`observation-final-20261002-01` 原记录仍为 paused/action_failed，TIMEOUT 的 children 阶段
808ms、初始化 21ms、桌面根获取 37ms；本轮不续跑、不改写旧计划或失败证据。

新增 `crates/actl-uia/tests/provider_probe.rs` 是默认 ignore 的手动诊断用例。
它把原生 `FindAllBuildCache`、数组 Length、逐项 GetElement 拆开留证，
可选择 CUIAutomation/CUIAutomation8、最小/NativeWindowHandle 缓存和新观察的 HWND。
不运行应用操作、不建立待恢复 flow、不写授权、不抢前台或恢复窗口，不引入依赖或 async。
目标查询前后核对 HWND/PID/class 与进程创建时间；这仅证明读到的原生字段一致，
不能证明期间没有发生同进程的窗口重建。桌面批量样本没有对应的单窗口身份保证。

### 证据与限制

本轮产物在 `target/provider-diagnose/`；诊断与已安装的发布版分开：

| 样本 | 客户端/范围/缓存 | FindAllBuildCache | 数组读取 | 结果 |
|---|---|---:|---:|---|
| root-minimal-01 | legacy / 桌面 / 最小 | 14,080ms | <1ms | 23 个元素 |
| root-handle-01 | legacy / 桌面 / 句柄 | 12,349ms | <1ms | 23 个元素 |
| root-legacy-handle-02 | legacy / 桌面 / 句柄 | 19,111ms | <1ms | 23 个元素 |
| root-modern-handle-01 | modern / 桌面 / 句柄 | 26,093ms | <1ms | 23 个元素 |
| window-chatgpt-legacy-01 | legacy / 单窗口 / 句柄 | 53ms | <1ms | 3 个子元素 |
| window-chatgpt-final-01 | legacy / 单窗口 / 句柄 | 37ms | <1ms | 2 个子元素 |

- 前两项诊断二进制 SHA-256 为 `715BF07DDE1E4D91A4B0140BB04788591DF4B2D1B82637E0B947650D95AF76BE`；
  中间三项为 `F3373DC680D01E5500D0FC0B7E89AEED4EA3524AF3942109AE158A0A1D05776D`；
  最终项为 `0FF4C05A50F41D54BC54E00B3F257991B3FAAAD26B9C0F4EF1DD6212A3BC0B84`。
  都是 debug 诊断程序，不作为发布 CLI 性能或业务验收。后两版只增加诊断记录与客户端对照选项；生产实现未变。
- 前两项缺少输入端点；带端点的桌面样本期间输入 tick 变化，modern 样本前台也变化。
  当前桌面不是受控静态现场，不能按这些单次耗时排序缓存或客户端性能，也不能据变化认定人工输入导致慢查询。
  两次单窗口样本的输入 tick/前台和原生进程实例均保持不变；子元素数量变化，不能作为同一冻结树的性能对照。
- legacy 客户端 QueryInterface(IUIAutomation2) 返回 E_NOINTERFACE；modern 可读取连接 2000ms、事务 20000ms。
  modern 的一次整体查询仍成功耗时 26,093ms，直接证明该配置不是这次整体枚举的 20 秒硬截止。
  不反推慢在哪个提供程序；同步调用的不同阶段和提供程序内部等待仍待拆分。
- 最小缓存也有长耗时，当前不支持“删除诊断句柄缓存即可解决”的结论；单窗口快样本支持
  优先减少全桌面依赖，但不证明某个其他窗口就是根因。本轮未复现原始 TIMEOUT，不能记已修复。
- Termius 单窗口尝试被原生预检拒绝（隐藏/最小化或不可用），stderr/stdout 保留于
  `window-termius-legacy-01.*.log`；未执行其 UIA 查询、未恢复窗口、未重跑。
  当时用例尚未在预检拒绝路径写 JSON，这一留证缺口已补齐；旧尝试不补造原生状态或覆盖原记录。
- 已安装显示端 PID 59012 保持运行，无应用窗口或其他任务资源创建，无需关闭业务资源。
  原生清单 `native.json` 的 572 个 HWND 不等于 UIA 的 23 个元素；UIA 可有无 HWND 或同 HWND 多个元素。
- 最终用例以无效 HWND 0 验证预检拒绝类的留证路径：`preflight-invalid-01.json` 保留
  blocked/native_preflight/NOT_ACTIONABLE，子测试退出 101 符合预期，未进入 UIA 查询。
  这只验证拒绝记录，不作为 Termius 验收或对其旧状态的补证。
- 最终诊断程序的环境、路径和哈希见 `environment.json`。针对性编译与 clippy 通过；
  `cargo xtask gate` 通过，日志为 `gate.log`。手动用例在普通门禁中仍 ignore，
  门禁通过不代表原始 TIMEOUT 已修复。发布程序与显示端策略均未更换。

### 适合当前项目的优化顺序

1. 起止动画演示使用已有 `inspect-desktop-native.yml`，无需 UIA 控件内容；真实应用任务继续使用 UIA。
   已知目标优先绑定新观察且经过身份核验的 HWND，使用已有 within/depth/limit 和 batch，
   控件变化后重新观察；不为了观察一个应用而先枚举所有应用。
2. 窗口清单的客户端/缓存请求复用、属性批量刷新与缓存读取已实现，验证见下节；
   生产 children 的原生查询与数组处理诊断也已拆分，2026-10-03 证据见下节。
   缓存错误必须传播，缓存不是授权或跨窗口长期身份依据。
3. 收敛只读客户端创建入口后再评估 CUIAutomation8 与明确的 provider 时限。
   本轮对照没有证明换客户端可以治好慢查询；不得顺便改变动作模式的焦点策略、输入授权或全局重试。
4. 需要整条观察的强时限时，再设计仅承载只读查询的短生命周期子进程隔离，使用已有 std 进程/留证能力，
   不引入 tokio、不把未返回当作成功空树、不隔离后盲目重放写操作。
   COM 对象不跨进程传递，返回序列化观察与身份，执行时重新核验；进程创建开销也需实测。
5. `list-windows` 当前完整 UIA 清单语义继续保持。若以后改为原生发现后按窗口读取或允许部分结果，
   必须单独 spec，核对虚拟窗口、owned dialog、同 HWND 多元素、UWP 辅助窗及失败/未知表达，
   不能静默用原生清单替代、略过失败窗口或把完整性错误清零。

手动运行方法（在新产物路径留证；可选客户端为 legacy/modern，mode 为 minimal/handle）：

```powershell
$env:ACTL_PROVIDER_PROBE_MODE = 'handle'
$env:ACTL_PROVIDER_PROBE_CLIENT = 'legacy'
$env:ACTL_PROVIDER_PROBE_OUTPUT = 'E:/explore/computer_cli/target/provider-diagnose/new-sample.json'
cargo test -p actl-uia --test provider_probe -- --ignored --nocapture --test-threads=1
```

设置 `ACTL_PROVIDER_PROBE_HWND` 前先重新观察原生清单，使用十进制句柄；未设置时观察桌面根。
用例拒绝覆盖已有 JSON，stderr 的 begin/finish 保留未返回阶段；同步 COM 没有整条调用硬时限，
人工运行须使用有界的外部进程留证器。输出 successful/observed 只证明这些读取完成，不是业务流程验收。

## 窗口缓存优化：实现与验收边界

后续实现只调整本次观察内部的重复工作，不替换 UIA 清单或放宽失败语义：

- `ChildrenQuery` 复用同一 UIA 客户端创建的 RawView 条件与缓存请求，桌面根及每个窗口的
  children 查询不再重复初始化客户端；每次瞬态重试仍重新获取桌面根。
- 窗口过滤和挂靠弹窗分类使用本次 `FindAllBuildCache` 返回的缓存 HWND/ControlType，
  原有完整遍历保留，必需字段读取失败仍终止清单，不忽略失败窗口。
- `window_metadata` 先读 Name，空标题仍直接跳过；有标题时使用一次 `BuildUpdatedCache`
  获取 Class/PID/HWND。逻辑测试中的读取请求由原先四次变为两次，不代表整条命令耗时减半，
  也不把同一提供程序内部的实际 RPC 数量当作已测量结果。
- HWND 为空的虚拟窗口保留；没有 live 读取兜底、跨调用元素缓存、客户端版本切换、
  新的超时或重试。不改变 JSON 字段、错误码、输入授权、急停或旧 flow 记录。

实现留证仍位于 `target/provider-diagnose/`：

- `metadata-before.log` 先复现旧读取方式的三项失败；修正后的五项纯逻辑回归通过
  （`metadata-after.log`），覆盖读取请求数、空标题、虚拟窗口、刷新失败和字段失败传播。
  `cache-uia-tests.log` 的 actl-uia 测试通过；随后增加的交互核对用例在普通门禁中默认 ignore。
- `cache-live-metadata-01.json/.log`：已有 ChatGPT 窗口 HWND 5508086、PID 44488，
  缓存窗口属性及两个直接子元素的 HWND/ControlType 均与实时读取一致；原生进程实例和
  UIA RuntimeId 在核对前后稳定。缓存读取计时 <1ms（JSON 整毫秒记 0），不能据此推算清单耗时。
  测试不激活窗口、不注入输入、不建立流程或应用资源，无需清理业务窗口。
- debug CLI SHA-256 为 `0A112C139F22B490CA3D1BBDAD54EF0F6203D41B7F39664B6860A60A4FED46B7`；
  `cache-inventory-debug-01.json/.stderr.log` 的完整 UIA 清单返回空（命令 1944ms）。
  同桌面旧发布 CLI 的 `cache-inventory-control-01.json/.stderr.log` 同样返回空（1824ms）；
  原生指定句柄查询仍可看到上述前台 ChatGPT 窗口。这是 UIA/原生观察矛盾，
  不能把空清单当作完整验收或优化效果；原记录保留，不用这些数字计算性能提升。
  旧发布 CLI 的 SHA-256 为 `D2BF80BFE99432A1D342B9EA07B13E7CC24F0BF56A24A3A10C260FBAB5C4FA5D`。
- 为判读上述矛盾，独立只读探针 `cache-root-independent-01.json/.log` 在原生
  `FindAllBuildCache` 返回 `UIA_E_ELEMENTNOTAVAILABLE`（`0x80040201` / -2147220991，
  映射 STALE_REF，11465ms）。输入 tick/前台端点稳定；未重跑该失败，不能与原 TIMEOUT
  合并归因，也不能认定没有发生其他提供程序重建。探针路径/哈希另存 `cache-probe-binaries.json`。
- 首次门禁 `cache-gate.log` 在 batch 部分失败透传契约失败：原测试按不存在的标题遍历桌面，
  期望 NOT_FOUND，却先遇到 TIMEOUT。将该离线契约改为 Windows 不可能存在的 PID 上界，
  原生清单证明缺失，保留 NOT_FOUND 和部分步骤断言；定向测试通过（`cache-batch-contract.log`）。
  不放宽生产错误、吞掉 TIMEOUT 或改动 golden；门禁失败的原日志保留。
- 复跑 `cache-gate-final.log` 的 workspace 测试通过，lint-packs 内发布构建因运行中的
  actl-signal 占用目标文件失败。已核对 PID 59012、窗口 HWND 81529818 与显示端路径，
  无 actl 调用执行后发送正常 WM_CLOSE 并等待进程退出，未强制结束、未清理停止或流程文件。
  `cache-display-close.json` 保存退出结果；其 path 字段因退出后读取 Process.Path 为 null，
  不将其冒充路径证据，路径核对来自退出前的进程查询与本次管理调用。

缓存核对用例的哈希与采样后 debug CLI 哈希见 `cache-environment.json`。
门禁会另行构建 debug CLI，不能把采样后的二进制哈希替代此前命令的实测哈希。

最终 `cargo xtask gate` 全量通过，日志为 `cache-gate-ready.log`；release CLI 与显示端
已构建于 `target/validation/release/`。显示端 PID 57084、epoch `s18dac25a2fdebc480`，
`cache-display-start.json` 与 `cache-display-status.json` 确认 ready/responsive=true。
发布路径和 SHA-256、停止代次及原任务状态见 `cache-installed-binaries.json`；
停止代次仍为 `s18daa5225ceedbdc0`，旧任务仍为 paused/action_failed，没有自动续跑。
该显示恢复验证不作为完整 UIA 清单或原始 TIMEOUT 的业务验收。

代码回归与局部缓存核对通过，完整清单的有效现场对照仍待完成；原始 TIMEOUT 未复现、未记修复。
没有续跑或覆盖 `observation-final-20261002-01`，也没有切换输入授权或清理历史停止状态。

## 2026-10-03：生产查询与数组处理分层（spec）

`observation_query` 在原有 children 外层内分别记录 find_all_build_cache、array_length、
array_elements；query_scope 区分桌面根与元素子节点，Length 成功后才记录 result_count，
转换阶段记录 processed_count 和零起始 failed_index。失败不返回部分树，负长度返回 INTERNAL，
0 与未知/null 分开。flow metadata 的阶段白名单、docs/06 与新增 golden 同步。
既有 32/8 日志预算、RawView 范围、瞬态重试、停止和授权保持不变。
最内层阶段排除自身采样/日志成本，外层包含嵌套成本，不能相加或据此计算优化比例。

本轮留证于 `target/observation-query/`：

- `before.log` 的六项测试先复现阶段缺失、失败索引缺失与负长度成为空树；修正后
  `after.log` 六项通过，`uia-tests.log` 的 actl-uia 测试通过。
  `metadata-before.log` 复现三个新阶段被删去；`metadata-after.log` 的 15 项协议测试通过。
- 默认 ignore 的 `actl-devtools/tests/observation_query_capture.rs` 只捕获一次生产 list-windows，
  使用已有 std 进程留证器，30 秒到期仅终止自有只读 CLI；不运行 flow、不激活应用或授予权限。
  正常捕获会保存完整时间线、原生前后上下文及二进制哈希；observed 只表示查询返回，
  不保证发现完整，也不是业务验收。时间线读取和前后留证不在这 30 秒截止内。
- debug 样本 `debug-01/`、任务 `s18dac3518f7bd74c0`：生产桌面 FindAllBuildCache 为 122ms；
  Length 为 1，GetElement 成功取得 1 个元素，子元素 Length 为 0，最终窗口清单为 0。
  由唯一一次 window_properties 成功及代码返回路径可知，该元素因空标题被跳过；
  这不是本次数组转换失败，也没有负长度或 TIMEOUT。该元素缓存 HWND 为 13242656，
  阶段中的原生 PID 为 null，之后原生指定句柄查询返回空；不能凭失效句柄推断具体提供程序。
  原生前后上下文仍可见前台 ChatGPT HWND 5508086/PID 44488，完整清单未记验收通过。
- 此次命令 duration 为 12381ms，trace 的 session.start 为 12020ms，其中 history.retention
  为 11975ms；进程捕获总耗时 13932ms。查询慢、启动清理慢和外部捕获耗时是不同层，
  不把它们都归为 UIA。采样期间存在 foreign_injected 活动且部分输入 tick 变化，
  不认定人工输入导致结果，也不作为静态环境性能对照。
- 随后已有可见 ChatGPT 的 `window-metadata-01.json/.log` 核对通过：HWND/PID/进程实例与
  RuntimeId 稳定，窗口属性及两个直接子元素的缓存值与实时值一致。元数据读取 1ms，
  仅是局部属性读取，不与完整清单或前日数字计算加速比。无输入注入、应用资源或清理。
- debug CLI 哈希为 `577d47d1dcac4d74191838dd559e842d9c0b02d21cdf14158d86d184a3a82fd7`；
  Windows 版本与显示端环境见 `environment.json`，测试程序路径/哈希见 `test-binaries.json`。
  旧任务和停止代次均保留，本轮没有续跑或覆盖历史失败。

最终 `cargo xtask gate` 通过（`gate.log`），包括前端检查、fmt、clippy、workspace 测试与
lint-packs。release 只读复核见 `live-release.log`、`release-01/`，任务
`s18dac4497cc3ac580`：桌面查询 264ms、Length=1、转换成功 1 个元素，最终清单仍为 0；
命令 461ms、历史清理 4ms。新阶段完整记录，无 TIMEOUT，仍不算窗口发现或业务验收通过。
debug 与 release 的启动清理耗时只说明此次采样不同，不据此归因或计算代码加速比例。
release CLI SHA-256 为 `A60FDCFBF040205589D1CCD9603243B2217A3D12F0A2E738FA896CAA7802D7CF`。

构建前核对显示端 PID 57084/路径/窗口并正常关闭（`display-close.json`），构建后启动
PID 46644、epoch `s18dac43fc9f84d800`。首次 `display-restored.json` 响应检查为
companion_unresponsive，保留该记录；随后 `display-restored-recheck.json` 确认
ready/responsive=true，没有循环重启。最终二进制路径/哈希及状态见 `installed-binaries.json`；
停止代次仍为 `s18daa5225ceedbdc0`，原任务仍为 paused/action_failed，没有续跑或授予权限。

当前能把这次空清单收敛到桌面发现/匿名元素路径，并确认经身份核验的 HWND 路线可读；
还不能确定导致桌面根只返回一个元素的提供程序或条件。原 TIMEOUT 仍未复现、未记修复。
后续应独立验证桌面根身份、窗口重建和客户端上下文，并单独评估历史日志清理的启动开销。
只读子进程强截止仍是待设计的生产策略，不把诊断捕获截止当作已实现。

## 2026-10-03：历史日志清理策略调整（spec）

按用户要求将 history 正常检查间隔从 1 小时改为 15 天，容量预算从 64 MiB 提高到
3 GiB；CLI 与显示端后台共用同一常量和跨进程标记。7 天过期判定、近期与活跃文件保护
保留，检查间隔内容量可继续增长。默认策略 golden 与 15 天前/到期边界回归同步。

留证于 `target/history-policy/`：`before.log` 与 `interval-before.log` 先验证旧策略不满足新契约，
`core-tests.log` 的 actl-core 测试通过，`gate.log` 的完整门禁通过。显示端 PID 46644 核对
路径/窗口后正常关闭（`display-close.json`），新 release 显示端 PID 58516、epoch
`s18dac54f7589f8500`；`display-restored.json` 确认 ready/responsive=true。二进制路径/哈希、
原暂停任务及停止代次复核见 `installed-binaries.json`，没有续跑或授予权限。

降低频率没有优化单次扫描，不能将此前约 12 秒的 history.retention 宣称修复。
当前实现仍逐文件尝试读取 calls 状态，并在低于容量预算时排序全部保留文件。
下一步宜分段计时，并评估仅删除候选读取活跃状态、仅超预算排序；这些优化尚未实现，
不预报性能比例，也不为验证策略主动清理或改写历史失败证据。
