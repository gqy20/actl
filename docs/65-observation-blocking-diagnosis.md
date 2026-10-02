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
   生产观察诊断进一步拆分原生查询与数组处理仍待完成。
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
