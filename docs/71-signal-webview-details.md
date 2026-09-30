# 详情页 TypeScript 内容层（2026-09-30）

用户授权在已提交的原生轻量化版本 `a51b918` 之后实现 TypeScript 详情页，并允许将直接 Rust 依赖上限从 8 调整到 9，新增 webview2-com；前端环境必须由 pnpm 管理。

## 实现边界

常驻横条、提示与交接卡仍使用原生绘制。独立详情窗口按需创建 WebView2，内容区使用 TypeScript/CSS；任务标题、关闭/最小化及底部适用的暂停、继续、核对、急停/修复按钮仍由原生 Win32 控件处理。未修改流程协议、桌面授权、暂停、急停或恢复语义。

页面只有读取 Rust 快照的通道，回传只允许 version=1 的就绪及 32–4096 DIP 高度消息，拒绝外部来源、额外字段与操作消息。正文使用 textContent，内嵌页面采用脚本哈希 CSP，禁止网络、HostObjects、开发工具、新窗口和后续导航。NavigateToString 首次加载的内部 data URI 单独允许，最终消息来源须为 about:blank；这点由实际宿主测试确认，不能凭浏览器预览推断。

运行时缺失、初始化超过 8 秒、布局/消息失败或页面进程退出，回退原生内容并展示诊断；不会自动下载 WebView2，也不会因失败授予权限。控制器在退出和失败时关闭，COM apartment 按 RAII 释放，profile 独立放在 SignalPaths/display-webview。

## 视觉与开发

任务名 18 DIP、较低对比度；状态 22 DIP，通过文字及低饱和色提示运行/暂停/完成/错误；进度 13 DIP，当前业务步骤 17 DIP 加粗，原因 13 DIP。摘要只有确有进度时才将第二行视为当前步骤，未知和停止说明不误用步骤样式。记录入口和记录正文进一步分级，共用左边缘；急停上方的原生与预览横线均移除，以留白区分操作区。

`ui/signal` 使用 pnpm 管理 TS/Vite，仅有两项开发依赖，无前端运行时框架。开发服务器仅绑定 127.0.0.1，七种样例可热更新预览；预览按钮不操作桌面。`pnpm build` 生成随源码入库的单文件 HTML，发布 EXE 用 include_str 内嵌，不依赖 Node、不启动本地 HTTP 服务。`pnpm verify` 校验类型、状态样例和嵌入产物一致性，纳入 xtask 门禁及 CI；产物固定 LF 防止字节比较受 checkout 行尾影响。操作说明见 [前端 README](../ui/signal/README.md)。

## 验证记录与限制

前端状态样例测试 3 项通过；显示端常规测试 64 项通过、6 项忽略。另显式运行 WebView2 宿主用例，使用隔离 profile 与自有窗口，无真实任务、不注入输入、不点击操作按钮、不改变授权：实际加载内嵌页面、更新状态、展开记录、自适应高度、保持展开状态、把 HTML 风格诊断作为纯文本显示、导出截图，最后注入失败并确认关闭控制器、回退状态及正常销毁宿主。截图在 target/webview-review/paused-page.png。

首轮导航限制错误以及隐藏窗口无法触发可见布局的失败保留在 target/webview-live.log、webview-live-second.log；修正后的可见自有宿主通过记录在 webview-live-visible.log。该通过不改写此前失败，也不能外推为屏幕阅读器、跨原生/网页键盘焦点、多屏 DPI、缺失 Evergreen 的非开发机回退已通过；这些仍登记在 ROADMAP。

首批完整 gate 通过，记录 target/webview-gate.log；移除横线后通过记录 webview-gate-final.log，文字微调后的最终门禁记录 webview-gate-polish.log，宿主复验通过记录 webview-live-polish.log。本机同环境同构建参数测量：actl-signal.exe 初始原生版本为 1,386,496 字节，初版 WebView 为 1,475,072 字节，最终微调版为 1,475,584 字节（较原始增加 89,088 字节，约 87 KiB/6.4%）；actl.exe 保持 5,429,760 字节。此数不包含系统共享 Evergreen 的磁盘/内存成本，不代表固定打包运行时体积。

替换显示端前核对进程路径、PID、窗口类、当前执行与输入锁，以正常替换消息关闭；重启通过 paired actl.exe display-start/display-status 核验 ready。执行代次与预授权文件哈希保持一致，没有续跑旧任务。最新替换的路径、SHA-256、环境与清理证据在 target/webview-display-before.json、webview-display-after.json，webview-size-before.json 为微调前 WebView 版的大小；初始原生大小取首次构建前 Get-Item 输出。

发布版详情窗口已启动并核对 HWND/PID/类，但 actl 的 snapshot/screenshot 按设计排除显示端自身；独立截图又因详情不在前台而拒绝，不能据此记发布版视觉验收通过。失败调用留在 target/webview-details-snapshot.json、webview-details-snapshot-ready.json、webview-details-shot.json；该窗口随后核对身份正常关闭，未点击流程按钮。独立宿主截图与浏览器预览仅覆盖各自所述范围。
