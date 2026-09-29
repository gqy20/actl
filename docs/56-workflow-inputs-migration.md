# 可复用 workflow 与迁移记录

> 历史记录：文中的 PS/Bash 入口已退役。当前原生/workflow 入口与未验收事项见 [迁移清单](61-native-tooling-migration.md)；本页原始实测结论保留。

日期：2026-09-29。关联 [暂停恢复](55-resumable-desktop-tasks.md)、[误让行](54-repeated-handoff.md)。

## 本轮实现

- flow-validate / flow-run 共用 inputs 解析器；输入为 UTF-8 YAML/JSON 字符串映射。
- 模板只在 args 使用 `{input: name}`；声明、默认值及边界见 docs/06。
- 完整计划在执行前预检并持久化，恢复不重读源文件。输入不能注入命令选项。
- VS Code 桌面步骤迁入 skills/actl/workflows/vscode-resume.yml；scenarios/vscode-resume.ps1 仅准备隔离文件/profile、输入文件和登记任务。Checkpoint 是测试专用模板副本。
- verify-text.yml 提供已有应用的完整文本核验；edit-value.yml 提供核对原值、ValuePattern 更新、核对新值。后者不代表任意应用均支持 ValuePattern。
- window_ready 不再对窗口根调用 UIA SetFocus；已经在前台时不请求激活，否则单次 SetForegroundWindow 并等待实际前台确认。具体输入控件焦点、人工接管与监听健康护栏保持生效。

## 证据

- target/activation-before.log：先复现前台窗口仍收到多余激活请求，1 失败、1 通过。
- target/activation-after.log：修复后两例通过。该单测不证明所有真实误让行均消失。
- 参数解析三组测试、CLI envelope golden、保存后输入修改不影响计划、三份模板预检均有自动测试。
- target/workflow-migration-gate.log：首轮完整门禁通过（之前 clippy 测试模块位置失败已修复）。补充 null 默认值拒绝后以 gate-final 日志为最终门禁。
- target/workflow-migration-evidence/readback-result.json：真实 VS Code 保留现场的只读模板 completed，1 次调用，完整文本匹配。未执行输入、保存或关闭。

## 迁移边界与待验收

- 新模板 VS Code 启动/输入真机验收另记，不沿用旧生成计划的通过结论。
- Word/Excel 原脚本还包含 ZIP/XML 落盘内容核验、进程实例及资源归属判定。当前 flow 不支持这些断言，不能删掉它们后仅靠窗口状态宣称迁移完成。
- Zotero 原脚本包含集合行精确比较与导出 JSON 内容核验；同样保留旧验收轨，待提供有类型的只读证据适配后迁移。
- edit-value 模板尚未完成真实应用写入验收；没有把新增模板计为 Word/Excel/Zotero 完整迁移。
- 暂停/待核对保留现场；输入未知结果不得自动重放。剩余监听误报需要按来源计数和逐调用证据继续排查。

## 最终门禁与本轮登记

- `target/workflow-migration-gate-final.log` 完整通过（file-lines、fmt、clippy、workspace tests、lint-packs、regression harness、打包验证）。
- 发布 CLI：`target/validation/release/actl.exe`；SHA256 `8A919C79A0033502E586E6DF6519B17D52748D3A2E31225A1F5F15B8CFC8DFF8`。已安装到用户 cargo/bin，哈希相同。
- 显示端沿用 protocol 8，就绪检查通过；SHA256 `E7D0549CF7C68AFB0155D69EE06568A215B6F2D98E66F557D779FE782B263246`。本轮没有修改显示端代码。
- Windows 10.0.26200.0；完整环境和逐调用输出在 `target/workflow-migration-evidence/`。
- 新模板直接登记任务 `vscode-resume-58c97ae540214a5b85cf5bf1b7484382`，独立 profile/file，等待用户点击继续；登记时三步 attempts=0。恢复沿用该 ID，不重复新建任务。
- 真实写入及改动后的窗口激活尚未取得本轮通过证据；暂停任务保留。

后续实现：四个完整场景及整批入口已迁移，文件断言和归属断言已补齐；本页保留当时的验收记录，最新范围及待验收项见 [57](57-application-workflow-migration.md)。
