# 84 · 微信朋友圈场景实测:盲区解析三件套 + 覆盖层查看器 + 像素断言

日期:2026-10-14 · 性质:场景实测轨(不进 CI;过程证据,结论沉淀于
[apps/wechat.yml](../skills/actl/apps/wechat.yml)) · 环境:Win11 25H2
(build 26200) zh-cn,微信 4.1.x(mmui),开发机交互桌面,150% DPI

关联:[设计蓝图 §3.4](06-design-blueprint.md) · [packs 契约](../skills/actl/PACKS.md)
· 本轮产品化:`image-regions` / `preview-map` / `verify-visual`

---

## 1. 赛道定位(一句话)

朋友圈是 mmui 自绘重灾区(缩略图/封面/头像零 UIA 结构),本轮把
「找得到(几何)→ 看得见(可视化)→ 验得实(断言)」三件事全部一等命令化,
盲区工作流从 agent 临时脚本升级为 propose→confirm→act→verify 四段链。

## 2. 盲区根因定谳(全通道排除)

「其他 RPA 能看到更多」假说的四通道真机排除:

| 通道 | 实验 | 结论 |
|---|---|---|
| UIA 树遍历 | walker 源码确认 TrueCondition(raw view);FromPoint 命中返回的 TimelineContentCell 与遍历同源,raw 子节点 0 | 结构不存在,非查询方式问题 |
| MSAA 旧通道 | oleacc 全树遍历 121 节点,与 UIA 同构(含 simple child ID) | 旧通道同样无子内容 |
| 读屏标志 | SPI_GETSCREENREADER 本就 True(actl 常驻 UIA 客户端已过条件暴露检测);置位前后树 304→304 | 展开态天花板即复合 name |
| 应用侧 | 微信 4.1.x 条件暴露(检测无障碍客户端才加载完整 provider);社区确认 3.9→4.0 框架改版 | 见 docs 调研,Hook 注入是唯一超集但违反使用条款,不采纳 |

事实:`mmui::TimelineContentCell`=帖子项,`mmui::TimelineAdNormalCell`=广告项
(同列表同复合 name,按类名区分);帖子 ListItem 的复合 name 含
作者/正文/「包含N张图片」/时间——元数据全在,缺的只是视觉子内容。

## 3. 三件套实测(同一 feed 面 956,610,1628,1090)

- **image-regions**(几何提议):10 区域,密度 0.1-0.93;独立 python
  参考实现交叉校验同构(±20px 网格对齐差);`--motion` 双帧差把自动播放
  视频帖与静止照片分开(单帧视觉咨询判读"photo"无法区分,实测 D 区
  motion 24.4)——视频/GIF 的定性依据是运动不是外观。
- **preview-map**(UIA 可见性画框):112/112 全量,42 交互框;右侧缩略图区
  零框=盲区判读的现场可视化。
- **verify-visual**(像素断言,本轮新增):朋友圈查看器闭环三态验收——
  p1 vs p2 --changed(mean_abs_diff 60.4/changed_ratio 0.94/17 域);
  p1 vs p1 --unchanged(0/0);违例方向 --unchanged → ASSERTION_FAILED
  带 metrics。时间戳重绘(每轮「N分钟前」)在胞阈 12 之下不触发。

## 4. 朋友圈查看器语义(覆盖层,非独立窗)

与聊天图片查看器(mmui::PreviewWindow 独立窗,UIA 全可见)是**两种形态**:

| 面 | 窗口形态 | UIA | 打开判据 | 关闭 |
|---|---|---|---|---|
| 聊天图片 | 独立窗「图片和视频」 | 全可见 | list-windows 新窗 | esc |
| 朋友圈帖子 | **窗内覆盖层** | 零新元素 | 区域像素大改(verify-visual --changed) | esc(只关查看器,主窗保留) |

翻页:left/right 帖内照片顺序导航,**两端钳制不回绕**(5 态循环矩阵:
right 落点 diff 0.2/钳制后 ≤1.5);打开态与翻页态呈现不同(全带均匀
diff 非顶部集中,「说明层」假说否证)——像素验证须同态基线。

## 5. 事故与教训

- **交叉校验陷阱**:image-regions --rect 首轮坐标偏移 bug 未被 python
  交叉校验抓到——参考实现复制了被测的同一换算逻辑,相关错误互相抵消。
  判例:交叉校验的参考实现必须独立推导坐标空间。
- **FromPoint 是全局命中测试**:微信不在前台时打到 ZCode 的 Web 内容上
  (UIA 树连 CSS 类名都全暴露)。判例:hit-test 探针前必须聚焦目标窗。
- **视觉咨询细粒度弃权**:几何分箱到 ≤60px 精度即低置信弃权;判别式
  协议的分工=分类归 decide、几何归确定性扫描,noul 是命题概率不是数值
  回归(坐标回归属协议误用)。
- **微信 esc 双效**:feed 页 esc 连主窗藏回托盘;查看器打开态 esc 只关
  查看器。本轮收尾即触发:主窗回到托盘,托盘图标 UIA 树又遇 XAML 退化
  (display_change_guard quirk),唤醒需 explorer 重启——验收现场如实保留。

## 6. 开放项

- 托盘唤醒遇 XAML 树退化(4 元素壳)——display_change_guard 的恢复
  路线(explorer 重启)未在本轮执行,留待下次真机会话验证;
- image-regions --motion 的真机阳性(实际视频帖)尚未采集:本轮视频帖
  滚出视野,判别逻辑由合成单测覆盖,见 ROADMAP 观察项。
