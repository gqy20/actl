# 应用复验（2026-09-29）

## 环境与证据

- Windows 10.0.26200.0；任务 `app-verification-20260929`，写入交接由用户点击确认。
- 二进制 `target/release/actl.exe`，SHA256 `F55B9E229F8E8E4E66B4BC3B45E2AA4EA345E9B7A3B611ED5F999D6E980E7D41`。
- 逐调用 stdout、stderr、耗时、结果及隔离数据保留在 `target/app-verification-20260929/`。以下目录均相对此目录，结论以各目录 `result.json` 为准。
- 原失败轮次不改写。浏览器投影通过不代表应用局部容器削减或所有 YAML 路线通过；未覆盖路线继续保留原状态与历史日期。
- `cargo xtask gate` 通过，日志 `target/app-verification-20260929/gate.log`；门禁通过与上表各真机结论分别报告。

## 结果

| 应用/场景 | 结果 | 证据目录 | 清理 |
| --- | --- | --- | --- |
| Chrome 动态页面 | 通过：40 项动态列表、稳定等待、歧义拒绝、中英文文本、四种投影 | `browser-chrome-a41e06335f584ddabab12eb13d7104c7` | closed_verified |
| Edge 动态页面 | 同上，通过 | `browser-edge-579be4470f154e9b8759edda4976f021` | closed_verified |
| VS Code 完整编辑 | 未通过整轮；三轮插入/撤销/重做及全文替换共 10 项磁盘全文检查通过，关闭重开阶段中断 | `vscode-edit-dfa09305ac2c4d7b98161f204f74500d` | finally 为 already_closed；不能据此宣称自动关闭通过 |
| VS Code 独立启动关闭 | 通过；新 profile/测试文件，只发送一次 close-window，轮询确认 HWND 消失及原进程实例退出 | `vscode-close-993b8cec9fad437a91a93d8bb5b1183f` | closed_verified |
| Word | 首次 type 在观察阶段报 STALE_REF / control_type / HRESULT -2147220991；写入未验收 | `word-edit-decc9c65baba4f09a53042e97aea8fd8` | closed_verified |
| Excel | TSV、公式表达式、重算、撤销、重做、区域复制 6 项通过；关闭阶段 snapshot 报 NOT_FOUND，重开未执行 | `excel-table-061735eb91e0488aa821ad1463c2cc21` | finally closed_verified |
| Zotero | 导入菜单动作返回成功，但等待后没有预期两条合成文献；搜索/导出未验收，原因尚未确定 | `zotero-search-f8c5557010454d7881a84651207de350` | closed_verified |

### VS Code 原始失败与人工操作

- `vscode-edit-e7d613626d0642cc837518b3e5616773`：启动后 find 报 STALE_REF，关闭即时检查报告 retained_close_pending。后续缺席观察不改写本轮结果。
- `vscode-edit-80a7d7478391479ca0bb4af197cf686e`：首次插入保存通过，Undo 的交接超时；错误明确 action_started=false，脚本按保守规则 retained_uncertain，没有重放 Undo。
- `vscode-retained-cleanup`：临时清理上下文漏传 case，未执行关闭。补齐后的 `vscode-retained-cleanup-recheck` 返回窗口在动作准备阶段改变。
- 用户随后说明手动关闭过 VS Code。上述结果包含人工操作，不能归因为自动关闭正常或异常。按用户要求使用全新隔离窗口完成独立启动关闭复验；该成功不替代完整编辑场景剩余步骤。

### 浏览器输出规模

同轮、同一隔离本地页面的 JSON envelope 文件字节数（并非 token 计数）：

| 浏览器版本 | full | skeleton | content | content+skeleton |
| --- | ---: | ---: | ---: | ---: |
| Chrome 153.0.8010.53 | 25864 | 12281 | 11115 | 5185 |
| Edge 154.0.4258.37 | 28644 | 14293 | 11447 | 5219 |

两者稳定等待均观察到 20 次内容变化、22 次采样，最终读回 lazy-item-40。四种投影均未截断。

## 脚本维护

- VS Code 与浏览器跨调用 ref 绑定其来源快照；不支持快照绑定的等待改用唯一语义定位。VS Code 编辑快捷键核验具体编辑器焦点，启动观察仅有界重试 STALE_REF，不重试写入。
- Word 快捷键断言正文焦点，粘贴改为指向正文的 type --paste。
- Excel 名称框 Enter 断言名称框焦点，选择后按起始单元格名称唯一定位，后续键盘动作断言该单元格焦点。
- 静态复现测试先证明旧脚本缺少绑定/焦点断言，再修订脚本；测试纳入 regress.Tests.ps1。静态通过不替代真机通过。

## 剩余验收

VS Code 关闭重开后的命令面板、转到行、查找替换与切换文件；Excel 关闭阶段观察竞态及重开读回；Word 启动观察稳定性与完整写入；Zotero 导入就绪及搜索导出；夸克、腾讯会议、网易云、微信及系统 fixture 尚未在本轮执行。局部容器与 provider 重建仍按 ROADMAP 独立验收。
