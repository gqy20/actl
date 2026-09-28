# 浏览器接入原型

Windows 上独立运行的同步 Python/Playwright 原型。仅操作本次启动的 headless Chromium、隔离 profile 和带随机任务标记的本地测试页面。

## 运行

依赖锁定在 `uv.lock`，可在仓库根目录执行：

```powershell
uv sync --locked --project prototypes/browser
uv run --project prototypes/browser python -m unittest discover -s prototypes/browser -v
# 由调用方将 ACTL_PROBE_BROWSER 配置为已安装的 Chromium/Chrome 可执行文件路径。
if (-not $env:ACTL_PROBE_BROWSER) { throw 'Set ACTL_PROBE_BROWSER to the browser executable path.' }
uv run --project prototypes/browser python prototypes/browser/probe.py --browser-executable $env:ACTL_PROBE_BROWSER --output target/browser-probe-unique
```

浏览器路径必须显式指定；脚本不安装浏览器。每次使用新的输出目录。也可复用已安装 Playwright 1.54.0 的 Python 环境。

结果写入 `report.json`，包含环境、源码/浏览器哈希、逐调用结果、PID、profile 和清理状态。stdout 保持空白，诊断走 stderr。退出码：成功 0、已有急停 77、其他失败 1。报告是实验产物，不是 actl JSON 协议。

## 验证路径

1. 启动独立会话，绑定 browser WebSocket endpoint 和精确 CDP target ID；另建同 URL 标签页作为干扰项。
2. 在不同端口的跨源 iframe 中，用 CSS 定位点击一次，核验 `saved:1`；再用 XPath 读取。主页面及干扰标签页应保持 `untouched`。
3. 显式断开适配器，确认后续读取失败；显式重连并只读核验，不重放点击。
4. 由启动器发送 `Browser.close` 优雅关闭自己的隔离浏览器，确认进程退出，再检查连接失效。

唯一匹配为硬条件。目标 ID、URL 或页面标记变化明确失败。写入开始后任何异常均标记 uncertain，当前适配器对象在显式重连后仍禁止再次写入，须先 `recheck`。离线测试包含 uncertain golden。

## 边界

- 检查真实 `%LOCALAPPDATA%/actl/stop-requested`；脚本不删除急停。启动后失败或停止时保留浏览器和 profile，报告清理状态，不使用强制终止。测试 HTTP 服务退出后，保留页面不能继续刷新访问。
- 任务标记只用于隔离测试归属检查，不构成生产授权机制。尚未接入 actl 人工交接、输入监听、流程持久化、YAML 或发布命令。
- uncertain 仅保存在对象内存，进程重启后不能自动续跑。定位与写入之间不是原子事务，导航竞态尚需正式设计。
- 当前只支持一层 iframe、CSS/XPath、文本读取和点击核验；异步业务结果没有轮询等待。不会自动重连、选择替代标签页或应用层重试写入。
- CDP 接入的能力低于 Playwright 原生连接；真实浏览器兼容性必须单独验收。连接式 `Browser.close()` 用于断开适配器；启动器用 CDP `Browser.close` 结束自己启动的浏览器。

依据：[CDP 接入](https://playwright.dev/python/docs/api/class-browsertype#browser-type-connect-over-cdp)、[连接关闭语义](https://playwright.dev/python/docs/api/class-browser#browser-close)、[iframe 定位](https://playwright.dev/python/docs/api/class-framelocator)。

验收记录见 [docs/34](../../docs/34-browser-connection-prototype.md)。
