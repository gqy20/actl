# 84 · 评测 VM 底座建设实录(eval-win11-26h2)

日期:2026-10-13 · 性质:基础设施实测记录(装好一台可复位的 Win11 评测 VM 的全流程与坑清单)
关联:[40 自动化测试与评估系统](40-automated-testing-evaluation.md)(§6 虚拟化选型/§8 主机规划) · [78 三方对比测试集](78-comparative-benchmark-suite.md)(执行底座) · [apps/vmware.yml](../skills/actl/apps/vmware.yml)(pack 沉淀) · [83 控件缺口评估](83-ctrl-form-phase-survey.md)

## 0. 成果速览

| 项 | 值 |
|---|---|
| VM | `eval-win11-26h2` @ `G:\vm\eval-win11-26h2.vmx`,已入 GUI 库 |
| Guest | Windows 11 企业评估版 26H2 zh-cn(x64),本地账户 `actl1` |
| 硬件 | 4 vCPU / 6144 MB / 64 GB NVMe / NAT(e1000e,2026-10-14 由 Bridged 改) / EFI+安全启动 |
| vTPM | Present+部分加密(`vmx.encryptionType="partial"`,密码见 G:\vm\eval-win11-26h2-credentials.txt) |
| Tools | 13.1.5 已装,vmtoolsd 在跑 |
| 宿主↔guest 通道 | vmrun 全凭据链通:copyFile 双向 / runProgramInGuest / captureScreen / snapshot·revert·deleteSnapshot / 共享夹(UNC) |
| ISO 归档 | `G:\vm\iso\26200.6584...CLIENTENTERPRISEEVAL...zh-cn.iso`(6.9 GB) |
| 遗留 | 无阻塞:外网已修(§4.6)/snapshot CLI 已复活(§4.7)/基线 `base-nat` 就位;涉网 verifier 受宿主代理栈影响(§4.6) |

## 1. 全流程(六阶段,可复放)

### 阶段 1:宿主准备(VMware Workstation 26H1u1)

- 安装位置 `G:\VMware\VMware_Workstation\`(用户自装);`app-locate vmware --register` 入 apps.json;
- **WHP 模式**:宿主 WSL2 常驻 → Workstation 走 Host VBS Mode,guest 内不可嵌套虚拟化;评测周期内不关 WSL2,否则旧快照恢复失败(CPU 特性校验);
- 服务族(authd/tray/usbarbitrator/NAT)安装自启,收尾判定不算残留。

### 阶段 2:取 ISO(浏览器轮,Broadcom 表单同款形态)

- Eval Center → Windows 11 Enterprise → 注册表单(9 字段,set-value 主通道+select 下拉)→ 语言区选**简体中文非 LTSC**(LTSC 无 Store)→ `Download now`;
- ISO 6.9 GB 落 `D:\C\Downloads`(用户 Downloads 被重定向,注意核实实际位置),归档复制到 `G:\vm\iso\`。

### 阶段 3:建 VM(必须走 GUI 向导)

**vmcli 声明式建壳已被证死路**:VM Create 产物不进 GUI 库(vmrun start 报通信错误)、Power Start 报 `__vmware__` 组、ConfigParams SetEntry 卡 vTPM 密码。**正解 = actl 驱动 GUI New Virtual Machine Wizard**(全程实证):

1. Home 页"Create a New Virtual Machine"卡片是**自绘**——Invoke 静默无效,必须 find-image 模板+物理点击(见 83 §二.3);
2. Typical → Installer disc image file (iso) 填 ISO 路径 → VMware 识别 Win11 自动注入 vTPM;
3. 命名/位置(G:\vm\);
4. **Configure Encryption 强制页**(Win11 TPM 硬检):选 "Only the files needed to support a virtual TPM"(磁盘保持明文可被 harness 读写)——密码框读回恒空,用 set-value 清零+覆写,两框须等长;
5. 磁盘 64 GB 单文件 → Customize Hardware(内存 6144 / **CPU 是 ComboBox:set-value 不生效,必须 type+tab 提交** / 网络 Bridged);
6. 取消"Power on after creation" → Finish,vmx 入库验收。

### 阶段 4:装 Windows(guest 内,键盘驱动)

- 上电 EFI Boot Manager → 选 **EFI VMware Virtual SATA CDROM Drive**(空硬盘无引导项);
- **焦点纪律**:VMware 窗口必须前台 + `Ctrl+G` 抓输入后才按键,否则 actl `press` 全部假成功(focus_verified=false 如实上报);
- OOBE:选"我没有 Internet 连接"走本地账户;**账户 actl1 / 密码 actl1**(见 credentials.txt);评估版"桌面激活"约 15 分钟。

### 阶段 5:装 VMware Tools(guest 内)

- VM 菜单 Install VMware Tools → guest 内 setup64.exe 走完默认向导 → 重启;约 3-5 分钟;
- 验收:guest 任务栏 Tools 图标 / vmrun runProgramInGuest 通。

### 阶段 6:vmrun 通道验收(宿主侧)

完整语法(实测):`vmrun -T ws -vp <vTPM密码> -gu <guest用户> -gp <guest密码> <子命令> <vmx> [args]`——**三凭据缺一不可,参数序陷阱见 §4.1**。

## 2. 宿主↔guest 四通道矩阵(实测)

| 通道 | 状态 | 语义与坑 |
|---|---|---|
| `copyFileFromHostToGuest` / `copyFileFromGuestToHost` | ✅ | 文件双向,评测灌题/收卷主通道 |
| `runProgramInGuest` | ✅ | stdout **不回传**(只回 exit code)——复杂命令走三段式:宿主写 .ps1 → copyFile 灌 → `-File` 执行(结果写 guest 本地)→ copyFile 拉回;程序路径必须全路径 |
| `captureScreen` | ✅ | 截 **guest 屏幕**(不依赖 Workstation 窗口焦点),1.5 MB PNG 实测;评测的独立观察眼 |
| 共享夹(UNC) | ✅(人工) | guest 资源管理器 `\\vmware-host\Shared Folders\hostshare` ↔ 宿主 `G:\vm\shared`;`net use` 盘符映射报错误 64(HGFS provider 不支持 net use 会话绑定,已知行为非故障);诊断共享在线性用 `VMwareHgfsClient.exe`(返回共享名,不受名字解析影响) |
| Snapshot 类 CLI(snapshot/revertToSnapshot/deleteSnapshot) | ✅(2026-10-14 判别后全通,§4.7) | 历史失败定谳为 Bash 内联调用形态污染,非密码/VM 问题;关机态建快照后 headless vmx 短持配置锁;**报错≠未生效,突变后必须 listSnapshots 核验** |

## 3. guest 内人工入口姿势

- 文件交换:资源管理器地址栏 UNC 直达(建议**固定到快速访问**免手输);或 guest 桌面建 UNC 快捷方式;
- 登录注意:VM 重启后停在锁屏,`-interactive` 类 guest 操作要求用户已交互登录;自动化场景用 runProgramInGuest(Session 0)不要求。

## 4. 排障实录(问题 → 根因 → 处置)

### 4.1 vmrun 三凭据与参数序(最贵的一坑)

- **症状矩阵**:"A password is required"(缺 -vp)/"Command requires valid user name and password"(缺 -gu/-gp)/"Unrecognized command: <vmx>"(vmx 位置错)/"A file was not found"(-vp 后参数被吞成路径);
- **根因**:26H1 的 AUTHENTICATION-FLAGS 必须在子命令前;子命令与 vmx 的相对位置因类别而异(GUEST 类同序,错放即 Unrecognized);
- **处置**:固定调用模板(§1 阶段 6),全部实测通过后才算通道就绪。

### 4.2 HGFS"假阴性"与工具层反斜杠吞噬(自踩 AGENTS 纪律)

- **症状**:宿主侧写 .ps1 探测脚本灌进 guest 跑,`Test-Path`/`net use` 全报 67/False,一度误判"共享夹没生效";
- **根因**:**Bash heredoc 内联写含 `\\` 的脚本,工具层把双反斜杠折叠成单反斜杠**落盘——AGENTS"含反斜杠路径的命令序列一律写 UTF-8 脚本文件执行,不内联"的又一实证(这次踩坑的是我自己);guest 收到的 `\vmware-host\...` 是非法 UNC,67 是脚本污染的假信号;
- **干净信号**:用户手动输双反斜杠 UNC → 能看到全部文件(共享在线);手动 `net use Z:` → 错误 64(provider 不支持会话绑定,非故障);
- **教训**:宿主写含反斜杠的脚本必须用 Python/PowerShell 文件写入(不经 Bash 内联);判定共享在线性的权威通道是 `VMwareHgfsClient`,不是 Test-Path。

### 4.3 Session 0 vs 交互会话(假阴性之二)

`runProgramInGuest` 默认在服务会话执行;HGFS UNC provider 与资源管理器行为按登录会话挂载。Session 0 内探测到 False ≠ 用户会话不可见;`-interactive` 可指定交互会话但要求用户已登录。评测 harness 的判定一律走"结果文件回传",不依赖探测侧信道。

### 4.4 密码框读回边界(向导 Configure Encryption)

ValuePattern 读回恒空(安全控件特性):`set-value --verify-value` 报 ASSERTION_FAILED 但写入有效(掩码点数核对);`type` 通道同边界。写入正确姿势 = set-value 清零 + 单次精确覆写,两框掩码点数必须一致,否则弹"密码不能为空/不匹配"错误框。docs/83 §二.2 已把"verify 三态区分"列为 Phase 1 契约修复项。

### 4.5 CPU ComboBox 的提交语义

"Number of processors" 是 ComboBox:set-value 写内部 Edit **不被采纳**(摘要栏不刷新);select 又因下拉项不物化失败;**正确通道 = type 值 + press tab(焦点离开触发 ValuePattern 提交)**,左侧摘要栏刷新才算生效。Win32 传统应用 ComboBox 通用形态。

### 4.6 guest 无外网(已修,2026-10-14 改 NAT)

装机期 Bridged 导致 guest 只拿到 169.254(APIPA)——本宿主 12 块网卡(物理有线口未插线、WLAN、TAP/Wintun×2/Sangfor×2/FlClash/ZeroTier/VMnet1/8),Bridged 自动选择器桥到不可用适配器。**修法 = VM Settings 改 NAT**(比 Configure Adapters 钉物理网卡更稳:不依赖任何物理网卡在位,宿主换 Wi-Fi/网线/地点都不用重配——评测基线的结构性稳定)。

2026-10-14 实测核验(vmrun 三段式带外探测):guest 拿 DHCP `192.168.92.128/24`(网关/DNS `192.168.92.2`);guest→宿主 ping VMnet8(`192.168.92.1`)通;ping 1.1.1.1 78–88ms、HTTP connect test 200——**外网全通,且出站透明继承宿主 FlClash 代理(DNS 应答为 fake-ip 段 198.18.x)**。三条含义:①Web 类题可用;②涉网 verifier 结果受宿主代理栈影响,代理开关状态进评测环境头;③NAT 网络在 guest 判为"网络/Public profile"——本轮已设 Private 并加 ICMP 放行规则 `ACTL-Eval-Ping`(profile=any),host↔guest 双向 ping 实测通;host→guest 的 TCP 入站(MCP)仍须按端口单独放行。

收口收尾:**base 快照过时问题已解决**——旧 `base`(2026-10-13 23:25,早于 NAT 生效 23:58)已删,新基线 **`base-nat`**(关机态建,NAT+Private+ICMP 规则全部入基线)于 10-14 经 CLI 就位(§4.7);guest 用户目录的历轮探测残件(probe*/diag*/hgfs* 18 个)在建立基线前已清理。

### 4.7 Snapshot 类 CLI 认证失败(已解,2026-10-14 判别矩阵收口)

**现象(10-13)**:同密码 `-vp`,stop/start/runProgramInGuest/captureScreen 全过,`snapshot`/`listSnapshots` 独报 "Authentication for encrypted virtual machine failed"。

**10-14 判别矩阵**(单轮多假设,全部经 PS 脚本文件通道):

- `listSnapshots` 记录密码(13 字符)直接通过——**不复现**;日志取证全史仅 2 次失败(vmware-2.log `VIX_E_AUTHENTICATION_FAIL` @10-13 23:23:59/23:24:09,1.5 分钟后 GUI 快照成功),此后跨两次 VMX 进程代、运行/关机两态 20+ 连续成功;
- 错误密码矩阵:`-vp` 给错值,Power/GUEST/snapshot **全类一律 "Incorrect password"**——vmrun 对每类严格校验,"Power 类借 GUI 凭据缓存顶包"假设证伪;
- 字节变体矩阵:记录值 ±CR/LF/首尾空格全拒——记录值干净,"向导掩码 15 点 vs 密码 13 字符=带残字符"证伪(掩码点数≠字符数,红鲱鱼);
- **定谳**:密码正确、VM 无故障、无缓存顶包;唯一支持假设=同会话 Bash 内联转义污染(§4.2 同窗实证的同类机制)。昨天的失败调用串已无法字节级取证,工程上也无需——**修复=调用形态收口**(宿主侧 vmrun 一律 PS 脚本文件、禁 Bash 内联),密码不重置,GUI 不再需要。

**全链路验证(评测复位循环整环一轮通过,本轮墙钟)**:stop soft(~12s)→ **关机态** `snapshot base-nat`(NAT+Private+ICMP+清理后的 guest 全部入基线)→ deleteSnapshot 旧 `base` → start(Tools 带外通道 ~5s 就绪,DHCP 拿回 192.168.92.128)→ 运行中 `revertToSnapshot`(exit 0,实现=自动硬关机+revert,终态关机)→ start 就绪;整环 67s。两坑入 pack:①关机态建快照拉起的 headless vmx 持 vmx 配置锁数十秒,紧跟的突变操作报 "Cannot read the virtual machine configuration file",且 **deleteSnapshot 报错≠未生效**(该次报错实际已删,后续 listSnapshots 证实)——突变类操作后必须 listSnapshots 核验再决定重放;②start 后 ~5s 的 "guest ready" 是 Tools 带外通道就绪(fileExistsInGuest 即通),非桌面就绪——VM 级操作可用,`-interactive` 与桌面依赖操作仍需用户登录(boot 探测:guest 固件不支持休眠,无 Fast Startup,均为真冷启)。

## 5. 与评测架构的关系(docs/78 执行底座)

- **宿主 = 监考方**:actl 驱动 VMware GUI + vmrun 做 VM 级动作(起停/快照/灌题/收卷/截图观察);
- **guest = 考场**:被测 agent(路线 A 时 guest 内还需拷入 actl.exe 单二进制 + skills/actl 包——copyFileFromHostToGuest 一条命令,无需工具链);
- **复位循环**(评测核心,**已全 CLI 验证**,整环墙钟 67s,§4.7):`revertToSnapshot base-nat`(运行中可直调,自动硬关机)→ `start` 接力(Tools 带外通道 ~5s 就绪)→ 灌题 → 跑 → `copyFileFromGuestToHost` 收卷 → verify-file 断言 → 还原;突变类操作(snapshot/delete/revert)报错后以 listSnapshots 核验,不信单一返回码;
- **观察眼分工**:宿主 actl `screenshot --app` 截 Workstation 窗口(依赖焦点);vmrun `captureScreen` 截 guest 屏幕(不依赖)——评测观察用后者。

## 6. pack 与凭据档案

- 应用知识沉淀:[apps/vmware.yml](../skills/actl/apps/vmware.yml) 10 条 quirk(vmrun 语法/GUEST 参数序/guest io 三段式/HGFS 可见性假阴性/焦点纪律/密码框边界/ComboBox 提交/自绘卡片/bridged 默认值/WHP 模式);
- 凭据与 VM 概况:`G:\vm\eval-win11-26h2-credentials.txt`(vTPM 密码/guest 账户/VM 概况,仓库外文件,勿入 git)。
