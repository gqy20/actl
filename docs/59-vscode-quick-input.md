# VS Code Quick Input 组件核对

2026-09-29，本机 VS Code，隔离任务 vscode-edit-04f492197d03427982353808c001eb4f。仅组件探查，不将手工探查补记为原 flow 步骤通过。

## 实际结构与操作选择

| 组件 | 本机证据 | 执行方式与核验 |
|---|---|---|
| 正文编辑器 | 完整文本读取；前 17 步中的输入/保存、重开读回已有证据 | 定位具体任务文件的 Edit，保留输入焦点护栏和磁盘全文核验 |
| F1 命令输入框 | Edit；ValuePattern；value/text 为 >，可直接设置查询 | set-value 后等待 quickInput_list 内唯一命令结果，再 Invoke |
| 行号框 | Edit；ValuePattern；初值 :；输入失败时 UIA current 是 ListItem list_id_3_0，expected 是 Edit | 设置 :2，读取 value 精确核验；激活 quickInput_list 中“转到第 2 行，第 1 列。按 Enter 键导航。”，再核验 status.editor.selection 为“行 2，列 1” |
| 文件选择框 | Edit；ValuePattern；结果行名含任务文件 basename、目录和“文件结果” | 设置任务文件完整路径，核验 value；在 quickInput_list 内按唯一任务文件名选择结果；打开后完整读取对应编辑器 |
| 查找/替换 | 两个 Edit，精确名称“查找”“替换”，均支持 ValuePattern；空 TextPattern 会回显提示名称而非空值 | 使用 value 通道，不用 text 判定空值；独立定位并设置，执行替换后核验全文 |

实际探查确认：行号结果 Invoke 成功，状态栏为行 2，列 1；文件结果 Invoke 成功，目标第二文件全文为 Second owned file 加换行。原正文未插入 GOTO 标记，旧任务仍保留待核对状态。

## 为何原方案反复失败

1. 把 Quick Input 当普通可聚焦 Edit，type 会 SetFocus 后检查具体元素身份；provider 返回的焦点实际为列表项，因此正确拒绝。加等待无法修正这个组件语义问题。
2. 无容器的全窗口 find 受到其他重建节点影响，实测 STALE_REF/control_type；限定 within quickInput_list 后成功找到唯一行。输入变化后不能沿用旧 ref/行序号。
3. 命令面板/Quick Input 是临时界面，窗口失焦会影响其可见性。一次 UIA 树仍列出行号框，但截图中面板已不显示；之后的严格可见查询可能 NOT_FOUND。诊断需要同时核对前台和现场，不把缓存树当可操作证据。
4. 再次运行 gate 与真机写入重叠，input lock 单测因真实执行器持锁而失败。此为测试调度错误，后续门禁与桌面写入串行，不修改输入锁护栏。

## 修正

- 三个 Quick Input 步骤保留 22 步业务结构，使用 ValuePattern 写值和 Invoke 激活结果。
- 命令结果与文件结果限定 quickInput_list；文件名选择器由隔离资源准备器产生，作为 file_result/second_result 输入。
- 每次内容变化后重新定位；有界等待只等待明确目标出现，不将固定睡眠当准备完成。
- 原精确键盘焦点护栏保持不变；不允许因同窗或同列表而向任意控件输入。
- 复现与探查输出均留在 target/上述任务目录，line-properties/line-value-readback/line-local-result/line-invoke/line-caret、file-input-properties/file-result/file-invoke/file-content、find-properties/replace-properties 等 JSON。

## 验证边界

- 本次探查执行器为 C:/Users/gqy17/.cargo/bin/actl.exe，SHA256 为 9299B8B0882064C16446FCA6DD08165D8BCF1B17E5FF0EDAE49C3ABD64BF6A26。
- 修正后的流程预检与定向回归通过；完整 cargo xtask gate 通过，日志 target/vscode-components-gate.log。
- 组件实测证明行号导航、文件结果激活可行；查找/替换本次只读取属性，未执行正文替换。
- 按本次组件分析约定，不重新启动整轮；原任务与现场保留。修正后完整 22 步及主动暂停体验仍待复验，不将组件探查记为端到端验收通过。
