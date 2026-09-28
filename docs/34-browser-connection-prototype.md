# 34 · 浏览器接入原型与验收

日期：2026-09-28。用户授权：提交 UIA 组合定位后，实现会话、iframe、一次写后核验与断连的接入原型。

## 实现

UIA 已提交为 `67ad72e`。浏览器原型位于 [prototypes/browser](../prototypes/browser/README.md)，
采用同步 Python Playwright，经 CDP 连接本次启动的隔离 Chromium。适配器 198 行、启动及验收脚本 201 行，另有离线测试和依赖锁文件。

绑定明确的 browser endpoint、target ID、URL 和任务标记；同 URL 干扰标签页不能代替目标。
支持一层 iframe 内 CSS/XPath 唯一定位、一次点击及文本结果核验。断连明确报错；重连须显式执行。
写入开始后的异常进入内存 uncertain 状态，核验前禁止重放。错误形状有离线 golden。

不新增正式 actl 命令或 YAML 字段，不修改 Rust envelope；尚未接入人工交接、输入监听和持久化恢复。
用户已有浏览器会话不属于该原型的操作范围。详细限制、运行方式及上游 API 依据见原型 README。

## 本轮证据

环境：Windows，Python 3.12.9，复用已有 uv 工具环境，Playwright 1.54.0。
候选浏览器来自本机 Playwright Chromium 缓存（构建 1208，chrome-win64），本轮未启动，兼容性未经验证。

被测源码 SHA256：

- `adapter.py`：`F680E1B15E7EAE0209A85640072C19AB3A92D06E276F3809FE29F44E84F70537`
- `probe.py`：`66534160D93ADDDC798CDE4E97D807E837A6405A5D9713CB2EACB2553ED9904F`

| 检查 | 结果 | 本地产物 |
| --- | --- | --- |
| 原型离线 unittest | 11 项通过，使用假 CDP/页面对象 | 测试入口 `python -m unittest discover -s prototypes/browser -v` |
| `cargo xtask gate` | 通过；Rust 205 项通过、4 项忽略，含回归脚本护栏检查 | `target/browser-prototype-gate.log` |
| 第一次实际启动请求 | 受阻，`ABORTED` / `stop_active` | `target/browser-probe-20260928-blocked/report.json` |
| 再次核对退出码 | 仍受阻，退出码 77 | `target/browser-probe-20260928-blocked-exit/report.json`、`target/browser-probe-blocked-run.log` |

两次受阻请求均在依赖加载与浏览器启动前退出：`calls=[]`、`checks=[]`、`cleanup=not_started`，未创建 profile、PID 或浏览器资源，现有急停未清除。未执行真实点击，未发生待清理浏览器。

## 尚待验收

以下为初次受阻时的历史待办；2026-09-28 后续隔离会话结果见文末，正式 CLI/YAML 边界仍未实现。

真实 CDP 会话绑定、跨源 iframe 点击核验、同 URL 标签页隔离、客户端断开重连和浏览器退出导致的断连均未通过真机验收。
依照 AGENTS.md，已有急停仅能按用户对本次停止的明确授权解除；本轮保留停止状态。后续验收须另建输出目录，保留本轮受阻记录。

`cargo xtask gate` 不包含该可选 Python 实验的测试，需单独执行上述 unittest。正式接入前仍需解决导航竞态、持久化 uncertain、人工接管和异步后置条件；不能据此宣称复杂浏览器 YAML 流程已经可用。

## Phase 2 无人值守复验（2026-09-28）

- 首轮 `target/phase2-20260928/browser-live/report.json`：精确会话绑定通过，iframe 写后核验失败并保留 uncertain，原报告不改写。随后只读检查目标 iframe 与主页面均为 `untouched`，没有重放点击；记录 `browser-recheck.json`。核对进程路径、PID、隔离 profile 和页面任务标记后正常关闭，单独清理记录为 `browser-cleanup-confirmed.json`。
- 新一轮独立会话 `target/phase2-20260928/browser-live-2/report.json`：精确 target、跨源 iframe 一次写后核验、XPath、同 URL 干扰页不变、客户端断开、显式重连只读核验和浏览器进程关闭导致的断连全部通过；`cleanup=owned_browser_exited`。
- 环境为 Windows、本项目 uv 锁定的 Playwright 1.54.0、Python 3.13.6、已安装 Chromium 1208。浏览器与源码哈希、逐调用证据在报告中。未安装新浏览器、未访问用户现有浏览器会话。
- 原型报告新增最多四层、每层最多 4096 字符的异常原因链；离线测试 12 项通过。首轮失败发生时未记录底层异常，根因不能根据第二轮成功推断。
- 上述结果仅覆盖隔离的 headless 本地测试页面，仍不证明人工交接、导航竞态、持久化恢复或正式 YAML 接入。
