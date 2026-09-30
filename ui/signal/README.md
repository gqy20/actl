# 任务详情前端

使用 pnpm 管理 TypeScript + Vite，零运行时框架。Node 24、pnpm 版本以 package.json 的 packageManager 为准。

```powershell
cd ui/signal
pnpm install --frozen-lockfile
pnpm dev
pnpm build
pnpm verify
```

开发页默认仅绑定 127.0.0.1，顶部切换暂停、运行、完成、待核对、未知、急停与长内容样例。预览按钮不操作桌面，不代表恢复/授权验收。Vite 提供热更新，修改视觉通常无需重编译 Rust。

`pnpm build` 把页面、CSS 和 JS 合并为 `crates/actl-uia/assets/signal-details.html`，必须随源码入库。`pnpm verify` 检查类型、状态样例、构建和嵌入文件一致性，不重写嵌入文件；已接入 `cargo xtask gate` 单点及 CI。

发布 EXE 内嵌此静态页面，不需要 Node/pnpm/Vite，也不启动 HTTP 服务。机器已有 Evergreen WebView2 才加载网页详情；运行时缺失、初始化超过 8 秒或页面进程失败时，保留原生详情和原生操作按钮。不会自动下载运行时。

WebView 仅渲染 Rust 发送的快照；回传只能是就绪和有界高度消息。页面不包含操作、授权或流程写入通道。暂停、继续、核对、急停和窗口关闭由原生 Win32 控件执行。禁止后续导航、新窗口、网络、HostObjects 和开发工具；文字使用 textContent，CSP 只允许嵌入脚本哈希。详情 profile 位于 SignalPaths/display-webview，与其他浏览器隔离。

原生宿主验收（会短暂展示自有测试窗口，不注入输入、不创建真实任务）：

```powershell
cargo test -p actl-uia --bin actl-signal web_details::live -- --ignored --nocapture
```

用例检查实际嵌入页面加载、状态更新、折叠高度、展开状态保留、纯文本展示、截图与失败回退。截图保存在 target/webview-review/paused-page.png。屏幕阅读器、键盘跨原生与网页控件的完整手动验收另行记录。
