# Windows 前台交接修正

## 原失败证据

2026-09-29 小红书试采：target/xhs-profiles-20260929-162543。
Chrome HWND 68100 可见且未最小化，前台仍为 ZCode HWND 985468。
press Ctrl+T 和 focus-window 均在 window_prepare 失败，未发送后续键盘输入。
任务已通过 user_start；旧实现未转交 Windows 前台资格，且丢弃 SetForegroundWindow 返回值。
这是实现缺口，旧证据不足以判定当时 Windows 拒绝的唯一原因。

## 修正

显示端在真实开始事件及同任务授权复用时，按调用转交前台资格。
核对当前调用 PID 前缀、同目录 actl.exe 路径、进程存活；持有句柄至 API 返回。
不向所有进程开放，不改变任务授权、暂停、急停或具体输入焦点检查。
Windows 拒绝转交时保留诊断，仍由实际前台状态决定是否能执行。
同调用不循环转交；新的 CLI 进程重新检查授权并独立尝试。
后续跨应用测试复现仅转交不归还仍失败：证据在 target/foreground-live/matrix-2.json。
最终采用显示协议 v9，将资格租约绑定完整动作 guard，最外层结束（含错误退出）归还显示端；进程内下一动作重新领取。
最小化同样必须归还，不能只覆盖 activate。另记录了显示线程短暂超过 500ms 的响应：只对授权检查做一次有界重查，不重复动作。

记录 foreground_transfer 和 window_prepare 原生请求返回值。不得以 SetForegroundWindow 的 LastError 推断原因。
不会因本次修改自动把单 CLI 包装成可恢复 flow；业务续跑继续使用现有 workflow。

## 验收

单测覆盖前台无需请求、后台单次请求、拒绝返回值保留和转交目标限制。
最终 debug 二进制路径 target/debug/actl.exe 和 actl-signal.exe，哈希见 target/foreground-live/binaries-final.json；Windows 当前交互桌面，Chrome HWND 68100、ZCode HWND 985468。
foreground-final-20260929 六个独立 CLI 调用通过：ZCode → Chrome → 最小化 Chrome → 恢复 Chrome → ZCode → Chrome。逐调用见 final-1.json 至 final-6.json。
history-final.json/final-summary.json：开始 1 次，资格转交 6 次、归还 6 次，均 accepted=true；只读查询重查 2 次。
原有网页内容未修改，无新增浏览器窗口、标签或需清理的应用资源；结束前台 Chrome。中途空白 Chrome HWND 332704 已消失，NOT_FOUND 保留，不记为通过。
以上只验收前台切换，不代表小红书采集或其他业务通过。人工接管和进程内嵌套动作的真机矩阵仍需补充；Windows 可拒绝合法请求，不能承诺无条件前台切换。

## 最终检查与安装

`cargo xtask gate` 通过，日志 target/foreground-v9-gate-final.log：file-lines、fmt、Clippy、workspace tests、11 个 pack/296 步静态检查均通过。中间新增测试模块放置触发 Clippy，已修正，原日志 foreground-v9-gate.log 保留。
release 构建安装至 C:/Users/gqy17/.cargo/bin/actl.exe 和 actl-signal.exe，源/目标 SHA256 一致（installed-hashes.json）。正常替换显示端后 installed-ready.json 确认 protocol=9、ready=true、stop_requested=false；执行代次保持 initial，没有续跑旧任务。
release CLI SHA256：68A0F5A5CF814A09C92BC5A0FCD6BF389E3024BA2D64C5F4A022622BE35EA796。
release 显示端 SHA256：8D532937D128A14A398935D42597A50C96D4BA7F605A82578CC75ADFD14B1866。
六步实测使用前述 debug 构建；release 本轮核验安装哈希与显示就绪，未重复六步业务验收。
