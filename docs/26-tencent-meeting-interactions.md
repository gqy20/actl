# 腾讯会议：开会、预定与云录制

后续补测：日期整段替换与跨月读回已验证，见 [第一批深测](27-batch1-deep-tests.md)。本文保留初轮验收边界。

## 范围与结果

2026-09-28，Windows 10.0.26200.0 中文桌面，腾讯会议 3.46.1.409。
使用项目 `target/validation/release/actl.exe` 与配套显示端。
本轮经 agent 观察、UIA 操作、显式 `--physical` 点击完成真实业务闭环；
pack 是分阶段操作知识，不能将坐标占位符填一次就视作无人值守脚本。

| 场景 | 实测结果 | 验证依据 |
|---|---|---|
| 开启会议 | 通过 | 快速会议进入会中；成员 1，解除静音/开启视频显示麦克风与摄像头关闭 |
| 预定会议 | 通过（默认日期/时间） | ACTL-test-20260928-01，09/28 01:00–01:30，首页出现及预定成功窗口 |
| 查看录制 | 通过 | 本轮录制进入详情，播放从 0:00 前进至 0:05，最后到 1:36 |
| 下载录制 | 通过（单流视频） | Chrome 显示完成；磁盘 MP4、ffprobe 与 SHA256 复核 |
| 任意日期/时间输入 | 未完成 | QDateEdit set-value 成功但读回不变，不能发布为可靠输入路线 |
| 多流/音频单独下载、他人录制 | 未测 | 本轮仅下载自己的无声测试录制 |

没有邀请其他成员或分享会议链接，没有打开麦克风、摄像头或屏幕共享。
已结束单人测试会议，取消本轮测试预定，恢复原本开启的“入会开启麦克风”并读回 On。
摄像头原本 Off，保持 Off。云端测试录制、自动产生的转写以及下载文件作为成果保留。

## 1. 快速会议与无声录制

1. `list-windows` 核对安装进程、PID、主 HWND。首页 UIA 缺失入口按钮，截图确定快速会议与设置入口。
2. 快速会议下拉是独立窗口，截图检查开启视频、使用个人会议号均未选中。
3. 设置的常规页提供语义 CheckBox：`name:入会开启麦克风` / `name:入会开启摄像头`。
   先记录原值，只在需要时切换。`verify --checked` 仅断言 On；断言失败中的 actual=Off
   可作为关闭证据，但必须检查错误码和 evidence，不能把任意失败当作关闭。
4. 视觉点击快速会议，等待同 PID 新会中窗口；截图核对成员 1、麦克风和摄像头状态。
5. 会中录制工具栏缺少 UIA；按当前截图 hover→截图→physical click。
   依次处理独立窗口：云录制→开启→您已静音→继续无声录制。
   本轮默认同时开启文字转写，后续出现独立转写条目。
6. 会中截图确认云端录制指示及“结束录制”。`close-window` 会弹出退出确认，
   还需选择“全员结束会议”；以会中 HWND 消失确认，不能只看 close-window 的 ok。
7. 会后云端处理完成才出现可播放文件。恢复临时改变的入会设置。

独立弹窗经常同名“腾讯会议”；用 PID/class/rect 和新 HWND 消歧，禁止模糊标题点击。
主窗截图不含独立弹窗。画面显示缩放后，坐标仍按原图像素及当前 rect 换算。
开启转写面板会改变会中窗口宽度，必须重新测量。

## 2. 预定、读回与取消

首页视觉打开预定窗口后，class 为 `schedule_meeting::ScheduleMeetingDialog`。
表单提供 Edit、Spinner、CheckBox、ComboBox 等 UIA 元素，优先使用语义定位：

```powershell
actl set-value 'name:请输入会议主题' 'ACTL-test-unique' --app hwnd:<schedule_hwnd>
actl verify --value 'name:请输入会议主题' 'ACTL-test-unique' --app hwnd:<schedule_hwnd>
actl verify --value 'name:开始日期' '2026/9/28' --app hwnd:<schedule_hwnd>
```

本轮关闭“添加到日历”，保留 30 分钟、中国标准时间、不重复，无邀请成员。
提交前截图核对全部字段。关闭日历使后续 ref 序号变化，不能沿用先前 @e43。
“预定”也同时匹配 Text 和 Button；实际提交按钮的 automation_id 为：

`QApplication.BaseDialog.DialogWidget.schedule_meeting_dialog_main_widget.QWidget.QFPushButton`

先 get 核对名称/角色，再 click 一次；以成功窗口和首页唯一主题、时间复核。
本轮清理路线：对应卡片更多菜单→UIA `name:取消会议`→确认框重新定位取消 Button→首页条目消失。
原有会议未操作；“预定成功，去分享”窗口只关闭，没有分享邀请。

### 日期控件限制

写入 2026/9/29 后，SetValue 返回 ok，界面和 `get` 仍是 2026/9/28。
方向键尝试改变了年段，读回 2027/9/28 后立即恢复并验证 2026/9/28。
这表明调用成功不能等同字段写入成功，且方向键需要精确知道当前段。
本轮最终仅采用已经核对的默认日期/时间；尚未断定是 Qt provider 还是控件处理问题，核心未做推测性修改。

### 可执行草稿检查

```powershell
pwsh -NoProfile -File scenarios/tencent-meeting-draft.ps1 `
  -ScheduleHwnd <刚观察到的预定窗口句柄> `
  -Topic 'ACTL-draft-unique' -ExpectedDate '2026/9/28'
```

脚本核对进程/窗口类，先验证日期，再填写并读回主题，最后再次读回日期和截图。
输出保存在唯一 `target/meeting-draft-*/result.json`，含逐调用 envelope、stderr、elapsed_ms。
脚本的 passed 只证明主题和日期检查通过，`submitted=false`；时间、时区、时长、重复、日历、邀请待观察。
不提交、不重放失败写入、不关闭调用方提供的窗口；退出 77 表示紧急停止，其他失败退出 1。
真实表单含账号信息，原始输出只留本地 target，不提交截图/录制文件。

脚本真机复验：

- `target/meeting-draft-c40a568e86244a51a6fb6177449147a1/`：7 次调用完成，主题/日期读回通过，未提交。
- `target/meeting-draft-477fa744854d4da9b5ec5116559e5d9c/`：故意传入错误日期，第三步拒绝，
  没有执行 set-value；另行读回确认主题仍是上次草稿。随后关闭测试草稿窗口。
- `target/meeting-draft-7096511fabe648909a71f2bd04cdb5b1/`：首次 list-windows 超过 30 秒，
  脚本保留失败报告并停止，没有写入。复核窗口后新建独立运行通过，未修改原失败结果。
  窗口枚举偶发延迟尚未定位原因，不据此归因于腾讯会议或作性能结论。

## 3. 查看与下载

1. 左侧录制进入列表，核对标题、日期、时长。本轮新增一个约 1:35 视频和一个转写条目。
2. 点击目标视频进入详情，确认创建时间/成员身份，再点击播放。实际时间推进，最终播放到 1:36。
   无声测试显示“内容不足，暂未生成总结”，与视频是否可播放是两项独立结果。
3. 右上更多→录制文件，弹层列出多流、单流（说话人+共享屏幕）、音频。
   本轮选择单流行的下载图标（界面约 1.77 MB）；不要误点相邻删除。
4. 客户端转交默认 Chrome。网页“正在下载录制文件”不能作为完成证据；
   Chrome 下载记录显示文件 1,817 KB、完成，再检查实际文件。
5. 下载位置取本机实际目录。本机 Windows 已把 Downloads 重定向到 `D:\C\Downloads`，
   `%USERPROFILE%\Downloads` 不是这次保存位置。可通过下载项“在文件夹中显示”确认。

文件验收：

- 大小：1,860,619 字节。
- ffprobe：96.43 秒，H.264 视频 2160×1080，AAC 音轨。
- SHA256：`BE357E366DC409DBEB79B8D787CB5ADC08AE15D924A66A4DE8ABFBD3B2740992`。
- 未下载原有长会议；没有把用户姓名、邮箱水印、会议号、鉴权 URL 写入 pack。

## 本机视觉证据索引

以下文件位于 `%LOCALAPPDATA%/actl/shots/`，仅作本机复核，不打包入仓库：

| 文件 | 证据 |
|---|---|
| shot_18d93b16d3db05940.png | 单人会议，音视频关闭 |
| shot_18d93b46e3e70d040.png | 云录制开启、转写面板出现 |
| shot_18d93b9feb88f7f80.png | 测试预定出现在首页 |
| shot_18d93bb5b5b175040.png | 测试预定取消，原有会议保留 |
| shot_18d93bb873b0c38c0.png | 本轮视频和转写已生成 |
| shot_18d93bcc23d565e80.png | 视频播放推进至 0:05 |
| shot_18d93bd628ab14f00.png | 可下载录制文件列表 |
| shot_18d93bf7d3355c340.png | 播放至 1:36 |

## 官方资料与后续边界

[如何使用云录制](https://meeting.tencent.com/support/topic/1851/index.html)、
[如何管理云录制](https://meeting.tencent.com/support/topic/1843/index.html)提供功能背景；
当前客户端的具体入口以上述真机结果为准。

下一阶段应补日期/时间分段控件的可靠输入与读回，以及动态控件身份校验。
保留视觉 agent 路径；本次不把坐标固化进核心，不增加第三方应用 CI 依赖。

## 交付检查

cargo xtask gate 通过：file-lines、fmt、Clippy、workspace 测试、9 个 pack / 165 个步骤及回归执行器离线测试。既有 3 个文件超过 800 行为警告，无新增门禁失败。

