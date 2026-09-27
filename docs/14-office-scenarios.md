# 14 · Office 场景实测记录(2026-09-27)

真实应用场景实测(非 CI fixture;测试纪律不破——系统应用 fixtures 照旧,
Office 属"场景实测记录"轨:人工执行、本文档为产出)。标的:本机 Microsoft
Office 家庭和学生版 2021 zh-cn(Excel/Word;WPS 未装,装后补测)。
三个局部闭环(docs/10:保存/字段填写/读取回填)的实测结论与操作路线。

环境注记:实测中途桌面进入锁屏(操作者经远程会话),键盘注入全链路不可用——
锁屏下 UIA 后台调用(快照/读/extract/Invoke)不受影响,该特性本身成为
重要实测发现(见 §4)。

## 1. Excel

### 读取回填 ✅(闭环打通)

- **网格形态(空工作簿)**:单元格**不作为 UIA 元素暴露**(自绘网格;快照
  5.4KB,仅名称框 id:1001 / 编辑栏 / 搜索框三个 Edit);
- **网格形态(有数据文件)**:数据区物化为 **DataItem(实测 1246 个)+ 1 个
  DataGrid**——网格是虚拟化的,空簿/滚动区外不物化;
- **`extract role:DataGrid` → `source:"grid-pattern"`**:GridPattern 直读路径
  **首次在真实应用触发**,43 行 × 28 列全网格结构化输出,行列标齐全,数据
  逐格正确(空单元格回填坐标名,如 "D2")——**老 Win32 报表之外,Excel 表格
  抠数直接可用**;
- 单元格 `name` = 坐标("A2")而非内容 → `find --name <内容>` 不可用,
  内容检索走 extract 后本地过滤。

### 字段填写 ✅(剪贴板直灌路线)

键盘路线两条受阻(见 §4 现象记录),实测有效路线:

```
actl set-clipboard "hello actl 123"      # 或从别处复制而来(US1 数据搬运形态)
actl type id:1001 "B2" --app Excel       # 名称框跳转(SetFocus 链正常)
actl press enter --app Excel
actl press ctrl+v --app Excel            # 灌入选中格
actl press enter --app Excel             # 退出编辑态
```

读回(效果断言,替代不暴露 Value 的编辑栏):

```
actl type id:1001 "A1" --app Excel && actl press enter --app Excel   # 跳走
actl type id:1001 "B2" --app Excel && actl press enter --app Excel   # 跳回
actl press ctrl+c --app Excel
actl get-clipboard        # → "hello actl 123"(实测逐字命中)
```

### 保存链 ⏸(待解锁桌面续测)

ctrl+s / F12 的行为在锁屏发生前未能归因完成(键被锁屏层吞没,见 §4)。
桌面解锁后待测项:新工作簿首存走 Backstage 视图还是另存为对话框
(Backcase 不产生新顶层窗口,`--expect window:` 断言不适用,需
`wait --element` Backstage 按钮形态);另存为对话框本身为 IFileSaveDialog,
actl 已有深度支持(金任务 2)。

## 2. Word

### 正文读取 ✅(TextPattern 闭环,中英文逐字)

```
actl get "role:Document" --app "<文档标题>" --property text
```

实测(COM 预造内容,见 §5):英文段与中文段均逐字命中
("季度报告 2026\r敏捷的棕色狐狸跳过了懒狗。")。**Chromium 之外,
Word 是 TextPattern 正文通道的第二个实证应用**。快照 14.7KB
(skeleton 仅削 8%——文档视图交互元素本就少,Ribbon 按钮是内容主体)。

### 字段填写 / 保存链 ⏸(待解锁续测)

type --paste 通道预期可用(记事本同款),保存链同 Excel(§1)待解锁验证。

## 3. 产品修复(实测直接产出)

- **`ensure_element_focus` 前置免扰**(action.rs):物理键盘焦点已落在目标
  窗口时直接放行,不再"找首个 Document/Edit 强行 SetFocus"——多 Edit 应用
  (Excel:首页搜索框/名称框/编辑栏)里旧启发式会把正确焦点搅到搜索框,
  后续按键全被吞。实测复现:修复前 press h/ctrl+s 无效果。

## 4. 现象记录(环境交互,如实归档)

- **锁屏护栏**(timing 文档同款现象再实证):物理前台="Windows 默认锁屏界面"
  时,键盘注入被 verify-then-inject 正确拒绝(错误信息直说前台是谁,零误注入);
  **UIA 后台调用不受锁屏影响**(快照/get/extract/Invoke 全程可用)——锁屏
  会话里观察类操作可继续,注入类必须等解锁;
- **Excel 编辑栏聚焦怪象**:SetFocus 编辑栏后物理前台标题读空(瞬时子窗切换),
  护栏按空标题拒绝——fail-closed 正确但导致"键盘直写编辑栏"路线不可用,
  剪贴板直灌路线(§1)是既定绕法;
- **launch 与商店/Office 应用**:`launch notepad`/`launch WINWORD` 找不到
  程序(别名/路径),场景脚本用完整路径直启——README/skill 值得补一条
  "Office 启动用完整路径"提示。

## 5. 方法论沉淀

- **COM 预造内容**:PowerShell `New-Object -ComObject Word/Excel.Application`
  在锁屏下可用,为读回测试提供内容受控的 docx/xlsx(注意:脚本需 UTF-8
  **带 BOM**,否则 PowerShell 5 按 ANSI 读脚本,中文在写入文件前就被污染——
  实测踩坑,与 actl 无关);
- 本记录的操作脚本为一次性(/tmp),复测按本文档命令序列重建。

## 6. 结论

Office(Excel/Word)的**读取侧全通**(TextPattern 正文/GridPattern 表格/
ctrl+c 单元格),**写入侧 Excel 闭环**(剪贴板直灌 + 名称框导航);
保存链与 Word 写入待桌面解锁续测(预计同金任务 2 的既有能力)。
Quicker 需求 Top 分类(资源管理器 ✓ / Excel 读写 ✓ / Word 读取 ✓)的
解析空白就此填补大半。
