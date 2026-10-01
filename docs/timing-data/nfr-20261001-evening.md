# NFR 验收数据(2026-10-01 晚间,正常负载)

二进制: target/validation/release/actl.exe (0.1.8)
环境: Intel i7-170 / 22 逻辑处理器 / Windows 10.0.26200 x64 / 正常桌面负载

## 冷启动(PRD §4:P50 < 50ms,P95 < 80ms)

| 方法 | P50 | P95 | 备注 |
|---|---|---|---|
| PowerShell Stopwatch(n=50) | **67ms** | **94ms** | 最接近 PRD 基线(2026-09-26 实测 P50 40.2/P95 53.9) |
| bash date 循环(n=100) | 193ms | 225ms | 含 bash+date 工具开销,非可比口径 |

**结论**: PowerShell 口径 P50=67ms 超 PRD 50ms 目标 ~34%,P95=94ms 超 80ms 目标 ~18%。
PRD 基线(40.2/53.9)当日负载可能更轻;当日为正常负载(timing 纪律,绝对值不跨日比)。
screenshot/profile 类重操作不在冷启动口径内。

## snapshot 常规应用(PRD §4:< 500ms)

| 目标 | P50(n=20) | 结论 |
|---|---|---|
| 记事本空文件 | **433ms** | ✓ 达标(<500ms) |

## 输出体积(PRD §4:默认单命令 < 2KB)

| 命令 | 字节数 | 结论 |
|---|---|---|
| status | 135B | ✓ |
| get(单元素) | 245B | ✓ |
| list-windows | 2523B | 超出(全窗口清单,非"常规单命令"口径) |

## skeleton 削减率(PRD §4:深单链 ≥ 70%,浏览器/小窗天然受限)

| 窗口 | full | skeleton | 削减率 |
|---|---|---|---|
| 记事本(空文件) | 4742B | 2593B | **45%** |

PRD 记事本基线 38%(2026-09-27)——本次 45% 略优(窗口内容可能略有差异)。
深单链(Electron/VS Code)≥70% 目标待深容器场景验证。

## timing profile(参照)

notepad-get: n=20 P50=217ms P95=292ms(含完整 CLI 调用链)
ms-settings-cold-snapshot: 134ms(a11y 惰性激活计入)
