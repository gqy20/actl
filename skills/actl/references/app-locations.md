# 本地软件位置查找指南

agent 在编写验收脚本或 YAML 流程前,按以下优先级查找目标软件的可执行文件位置。

## 第一步：查 app-registry（最快）

```bash
actl app-list
```

如果目标已注册（如 `edge`、`paint`），直接用别名：

```bash
actl launch edge
actl browser-launch --browser-executable edge --url ...
```

## 第二步：常见位置探测

未注册时按以下顺序找：

| 优先级 | 位置 | 命令 |
|---|---|---|
| 1 | PATH 环境变量 | `where.exe <name>` |
| 2 | Windows App Paths 注册表 | actl 的 `launch` 已内置此解析 |
| 3 | `C:/Program Files/` | `ls "C:/Program Files/<厂商>/<应用>/"` |
| 4 | `C:/Program Files (x86)/` | 同上 |
| 5 | Store 应用（WindowsApps） | PowerShell `Get-AppxPackage <名称>` |
| 6 | 开始菜单快捷方式 | `ls "$env:APPDATA/Microsoft/Windows/Start Menu/Programs/"` |

## Store 应用的特殊处理

Store 应用路径带版本号，**每次更新会变**：

```
C:/Program Files/WindowsApps/Microsoft.Paint_11.2605.81.0_x64.../PaintApp/mspaint.exe
                                        ^^^^^^^^^^^^^^ 版本段，更新后变成新数字
```

**正确做法**：注册为 pattern（通配符），让 actl 自动重解析：

```bash
actl app-register paint \
  'C:/Program Files/WindowsApps/Microsoft.Paint_*/PaintApp/mspaint.exe' \
  --pattern
```

之后 `launch paint` 始终可用，无需关心版本号变化。

## 注册后使用

```bash
# launch 直接用别名
actl launch edge

# browser-launch 也接受别名
actl browser-launch --browser-executable edge --url about:blank

# YAML 流程的 inputs 里用别名
# inputs:
#   browser_executable:
#     default: edge
```

## 移除注册

```bash
actl app-remove my-tool
```

只删注册记录，不动应用本体。未知别名返回 NOT_FOUND。
Store 应用卸载后应清理对应的 pattern 注册。

## 纪律

- **先查再写死**：任何脚本/流程中的 exe 路径，先 `app-list` 确认是否已注册
- **发现新软件就注册**：找到路径后立即 `app-register`，下次不用再找
- **Store 应用必须用 pattern**：写死版本号路径是定时炸弹
- **注册写错或软件已卸载**：`app-remove <别名>` 清理，不手动编辑 apps.json
