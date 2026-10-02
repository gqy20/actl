# Workflow 资源初始化替代准备脚本

2026-09-29。用户要求去除运行应用测试前的外置 PS 准备层。

## 直接运行

```powershell
actl flow-validate --file skills/actl/workflows/vscode-edit.yml
actl flow-run --file skills/actl/workflows/vscode-edit.yml --id vscode-001
```

从头运行需要新的 ID。恢复使用 flow-resume --id vscode-001，不重跑初始化。Word、Excel、Zotero 分别替换模板文件名；verify-applications.yml 是共用一次任务交接的 64 步整批流程。vscode-resume.yml 也可直接运行，无需原来的准备脚本。若要验证输入前暂停，在专用计划中声明 pause_before，不用脚本改写流程文本。

## 实现与边界

- 通用资源层位于 actl-flow；CLI 完整解析输入、资源与所有命令，再持有运行锁创建资源和保存计划。软件路径、选择器和种子内容均在 YAML 或随包资产中；没有软件名分支，没有新增依赖或 shell 执行能力。
- 资源位于 actl 状态目录的 flow-resources/<ID>。新目录独占创建，文件 create_new；已有资源目录拒绝覆盖。初始化失败保留已创建资源，不自动补写。未登记 flow 时先检查资源目录，再用新 ID 重试；已登记的流程恢复只用冻结计划，不重置资源。
- flow-validate 读取复制源并验证大小，不创建资源或流程记录；上下文使用 validation-preview，返回 resources 清单。emit-plan 的资源路径是预检示例，不是已初始化的运行计划。
- 输入文件仍可覆盖默认值，但不会重定向资源写入；使用外部文件路径时必须自行提供文件并核对归属。
- 删除 prepare-workflow.ps1、verify-applications.ps1、office-package.ps1、vscode-resume.ps1。历史验收记录保留；其他独立观察/浏览器场景脚本不在此次资源层迁移范围内。
- Office 初始资产为本项目生成的最小 OOXML 文件；正文、单元格及损坏文件拒绝由现有验证器测试覆盖。只读 OOXML/进程检查器的内部固定 PowerShell 实现未改写，它不承担启动或资源准备。
- 合并流程为显式 YAML；一致性测试要求步骤、输入和资源与四份单软件模板一致，避免删除合并脚本后静默漂移。

## 验证

定向测试覆盖初始化仅一次、预检无写入、命令预检失败不创建资源、恢复不读取源模板也不重置编辑结果、路径越界/重名/文件与目录冲突拒绝，以及所有应用模板无需 inputs 文件即可预检。契约 golden 记录 resources 预检 envelope。

本次资源初始化与只读核验测试不启动真实应用，不作为 VS Code 22 步或四软件 64 步真机验收。此前失败记录见 docs/58 和 docs/59。

完整 cargo xtask gate 通过：target/workflow-resources-final-gate.log，包含发布包资产完整性检查。安装路径 C:/Users/gqy17/.cargo/bin/actl.exe 已更新，SHA256 为 88C235CAF4A447D33124761D631B3E8435A3E1838714B760C3B438F90B6EFE31；显示端同目录 CLI 同步更新，未启动桌面任务。安装后二进制预检 VS Code 模板为 valid=true、22 步，证据 target/workflow-resources-installed-validation.json；安装记录 target/workflow-resources-install.json。

## 持久浏览器 profile 资产（2026-10-02）

`resources.browser_profiles: [{name: <1-64 [A-Za-z0-9_-]>}]` 声明持久浏览器 profile（CDP 轨，docs/72）：

- **全局命名资产**，不属于 run 目录生命周期——创建幂等（`%LOCALAPPDATA%\actl\browser\profiles\<name>`），任务结束**永不删除**；恢复/清理/flow-clean 不触碰。
- **声明治理**：flow 中 `browser-launch --profile <name>` 只允许使用已声明的名字，未声明在预检即 PROTOCOL 拒绝（凭据资产显式归属；直接 CLI 使用不受此限）。
- 与 run 目录资源同受 128 项总量上限；summary 以 `browser_profiles` 上报。

定向测试：名称校验/重名拒绝/创建幂等/无声明根缺失容错；契约测试覆盖未声明拒绝与已声明放行。登录暂停点组合用法见 docs/06 §3 浏览器会话节与 `skills/actl/workflows/browser-login-session.yml`。
