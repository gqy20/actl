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

### 保存链 ✅(原地保存正链打通;场景框架修正)

真实场景 = **打开已有文件 → 修改 → ctrl+s 原地保存**——原地保存不弹任何窗口,
断言走磁盘终验而非 `--expect window:`(实测纠正:对未落盘的草稿 Book1 等
"另存为弹窗"是错误的场景构造)。实测全链:

```
actl type id:1001 "E1" --app <xlsx标题> && actl press enter --app <xlsx标题>
actl press ctrl+v --app <xlsx标题>          # 剪贴板直灌(§1 路线)
actl press enter --app <xlsx标题>
actl press ctrl+s --app <xlsx标题>          # 原地保存,无弹窗
# 终验:COM GetActiveObject 读运行实例 E1="saved by actl",文件 mtime 更新(实测双中)
```

**受保护视图(重要发现)**:TEMP 等不可信位置的文件,Excel/Word 以受保护视图
打开——UIA 读写正常,但**编辑被静默拒绝**(ctrl+v ok 却不落格、保存命令沉默)。
这是安全特性正确工作,actl 不绕过;正解 = `click name:启用编辑` 解除后正常操作。
F12/ctrl+s 在草稿 Book1 上沉默的另一半即此族现象(策略层禁用,非注入失败)。

## 2. Word

### 正文读取 ✅(TextPattern 闭环,中英文逐字)

```
actl get "role:Document" --app "<文档标题>" --property text
```

实测(COM 预造内容,见 §5):英文段与中文段均逐字命中
("季度报告 2026\r敏捷的棕色狐狸跳过了懒狗。")。**Chromium 之外,
Word 是 TextPattern 正文通道的第二个实证应用**。快照 14.7KB
(skeleton 仅削 8%——文档视图交互元素本就少,Ribbon 按钮是内容主体)。

### 字段填写 ✅ + 保存链 ✅(全链闭环,2026-09-27 解锁后实测)

```
actl type "role:Document" "actl 在此处追加了一行。" --app <文档标题> --paste
actl press ctrl+s --app <文档标题>
actl get "role:Document" --app <文档标题> --property text   # TextPattern 读回
```

实测:delivered:14 → ctrl+s 原地保存 → TextPattern 读回含追加内容且原文完整;
COM GetActiveObject 磁盘终验 contains=True + mtime 更新(双中)。**Word 三闭环
全通(读/写/保存)**,Excel 同(§1 写读 + 保存链)。注意追加落在焦点处
(文档头),定位插入点需先 click 目标位置或用名称框类导航。

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

Office(Excel/Word)**三个闭环全部打通**:读取(TextPattern 正文/GridPattern 表格/
ctrl+c 单元格)、写入(Excel 剪贴板直灌+名称框导航;Word type --paste)、保存
(原地 ctrls + 磁盘终验,双应用实测)。两个环境发现入档:受保护视图(不可信
位置文件的编辑静默拒绝,解法 click 启用编辑)、锁屏分层(UIA 后台可用/注入
等待解锁)。
Quicker 需求 Top 分类(资源管理器 ✓ / Excel 读写 ✓ / Word 读取 ✓)的
解析空白就此填补大半。
