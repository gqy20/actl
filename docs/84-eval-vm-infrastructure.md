# 84 · 评测 VM 底座建设实录(eval-win11-26h2)

日期:2026-10-13 · 性质:基础设施实测记录(装好一台可复位的 Win11 评测 VM 的全流程与坑清单)
关联:[40 自动化测试与评估系统](40-automated-testing-evaluation.md)(§6 虚拟化选型/§8 主机规划) · [78 三方对比测试集](78-comparative-benchmark-suite.md)(执行底座) · [apps/vmware.yml](../skills/actl/apps/vmware.yml)(pack 沉淀) · [83 控件缺口评估](83-ctrl-form-phase-survey.md)

## 0. 成果速览

| 项 | 值 |
|---|---|
| VM | `eval-win11-26h2` @ `G:\vm\eval-win11-26h2.vmx`,已入 GUI 库 |
| Guest | Windows 11 企业评估版 26H2 zh-cn(x64),本地账户 `actl1` |
| 硬件 | 4 vCPU / 6144 MB / 64 GB NVMe / Bridged(e1000e) / EFI+安全启动 |
| vTPM | Present+部分加密(`vmx.encryptionType="partial"`,密码见 G:\vm\eval-win11-26h2-credentials.txt) |
| Tools | 13.1.5 已装,vmtoolsd 在跑 |
| 宿主↔guest 通道 | vmrun 全凭据链通:copyFile 双向 / runProgramInGuest / captureScreen / 共享夹(UNC) |
| ISO 归档 | `G:\vm\iso\26200.6584...CLIENTENTERPRISEEVAL...zh-cn.iso`(6.9 GB) |
| 遗留 | guest 无外网;Snapshot 类 vmrun 命令 CLI 认证失败(见 §5) |

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
| Snapshot 类 CLI | ❌ | `snapshot`/`listSnapshots` 带 `-vp` 报 "Authentication for encrypted virtual machine failed"(同密码对 Power/GUEST 类全过)——见 §5 |

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

### 4.6 guest 无外网(TCP False,遗留)

Bridged 模式但 guest 只拿到 169.254(APIPA),`Test-NetConnection` False——宿主多网卡场景 Bridged 虚拟网卡可能桥到错误物理适配器。**不阻塞离线评测题**;Web 类题面前修:VM Settings → Network Adapter → Bridged → "Configure Adapters" 勾选正确的物理网卡(或改 NAT)。

### 4.7 Snapshot 类 CLI 认证失败(遗留,开放)

同密码 `-vp`:stop/start/runProgramInGuest/captureScreen 全过,`snapshot`/`listSnapshots` 独报 "Authentication for encrypted virtual machine failed"。疑点:向导密码框掩码 15 点 vs 密码 13 字符(type 注入前框被 set-value 写过,可能带残字符)——Power/GUEST 认证或借了 GUI 会话凭据缓存,snapshot 分支强制比对原始密码。**当前快照走 GUI 手打**(必然成功,读凭据管理器);后续修复路径:GUI 里重置加密密码为确知值 → CLI 全链路复活;若 revertToSnapshot 也拒绝,评测"秒级复位"支柱受损,此项升优先级。

## 5. 与评测架构的关系(docs/78 执行底座)

- **宿主 = 监考方**:actl 驱动 VMware GUI + vmrun 做 VM 级动作(起停/快照/灌题/收卷/截图观察);
- **guest = 考场**:被测 agent(路线 A 时 guest 内还需拷入 actl.exe 单二进制 + skills/actl 包——copyFileFromHostToGuest 一条命令,无需工具链);
- **复位循环**(评测核心):`revertToSnapshot base` → `start` 接力 → 灌题 → 跑 → `copyFileFromGuestToHost` 收卷 → verify-file 断言 → 还原——**依赖 §4.7 修复后才能全 CLI 化**,当前以 GUI 快照过渡;
- **观察眼分工**:宿主 actl `screenshot --app` 截 Workstation 窗口(依赖焦点);vmrun `captureScreen` 截 guest 屏幕(不依赖)——评测观察用后者。

## 6. pack 与凭据档案

- 应用知识沉淀:[apps/vmware.yml](../skills/actl/apps/vmware.yml) 10 条 quirk(vmrun 语法/GUEST 参数序/guest io 三段式/HGFS 可见性假阴性/焦点纪律/密码框边界/ComboBox 提交/自绘卡片/bridged 默认值/WHP 模式);
- 凭据与 VM 概况:`G:\vm\eval-win11-26h2-credentials.txt`(vTPM 密码/guest 账户/VM 概况,仓库外文件,勿入 git)。
