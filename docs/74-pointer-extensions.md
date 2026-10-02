# 74 · 指针扩展（轨迹拖拽/坐标框选/长按）实现与验收

日期：2026-10-02。性质：实现记录 + 场景实测轨（不进 CI 的真机证据在 `target/p3-paint-accept/`、`target/p3-cross-accept/`）。
关联：[06 §3 交互](06-design-blueprint.md) · [ROADMAP M3 指针与输入扩展](ROADMAP.md) · [31 身份与焦点](31-element-identity-input-focus.md)

## 实现（三项，全走物理注入面）

1. **轨迹拖拽 `drag --path X,Y ...`**：路径点（绝对物理像素，≤64，可重复传）从 start → waypoints → destination 按 **8px 线性插值**成一批 MOVE 事件（总事件 ≤32768 上界，超限 PROTOCOL）。插值器为纯函数（`mouse::interpolate_path`，离线单测覆盖端点精确/步长上界/退化输入）。无 `--path` 的既有 drag 保持原四事件直线行为**零变更**。
2. **坐标框选 `drag xy:x1,y1 xy:x2,y2 --app <窗>`**：坐标对坐标直连（marquee 语义）；两端均过**窗口边界守卫 + 遮挡守卫**（与坐标点击同源），准备期复核窗口未动、路径点在窗内；`--app` 必填。路由规则：from/to 皆 `xy:` 或带 `--path` 即走坐标路径（`PointerSpec::DragPath`）。
3. **长按 `key-down --duration Ms`**（50-60000，clap 范围校验）：按下 → 100ms 分片睡眠（每片过停止代次核验）→ 自动抬起。急停即释放并报 ABORTED（消息如实说明已释放）；key-up 不接受 duration。

## 测试与真机证据

- 离线：插值单测（actl-uia）；契约 5/5（`input_target_contract`：--path 解析错误/xy 对缺 --app/duration 边界/key-up 拒 duration，全部在桌面交互前拦截）。
- 真机（Paint，2026-10-02 `target/p3-paint-final/`，用户授权桌面时段）**三项正路径全通过**：轨迹拖拽（zig-zag `--path` 两路径点，`mode:"path"` 载荷）、坐标拖拽（`xy:450,880 → xy:1350,980`，前后截图字节差 155930→153333 证新笔画）、长按（`key-down b --duration 400`，`held_ms:400` 自动释放）。截图证据 `paint-strokes.png`。
- 负路径证据同样保留：遮挡守卫对被覆盖坐标的多次拒绝（`p3-cross-accept/`）——其中一次"探测说暴露、执行说遮挡"的假矛盾由 **DPI 坐标系差**解释：非 PMv2 观测脚本的 (x,y) 是虚拟坐标，与 actl PMv2 物理坐标不同指；actl 侧守卫始终正确。教训：外部观测脚本核对坐标时必须同为 PMv2 口径（list-windows rect 是物理口径，勿与系统 DPI 工具混用）。

## 过程教训

1. **launch 的 `--wait-window` 是 actl 侧旗标不是应用参数**——误传导致 Paint 打开"找不到 --wait-window.png"模态框；按前台弹层纪律先 `list-windows --native --hwnd` + `snapshot` 读全文（确认属清理白名单的「找不到文件+确定」形态）再 close-window 解锁。脚本传参应只传应用自身参数。
2. 显示端伴侣长期未跑 + `target/release/actl-signal.exe` 陈旧（gate 发布构建走 `target/validation` 目录）会造成 lease 失配循环：`display-start` 反复拉起旧二进制并锁住 exe。解法：停掉运行中的 signal → `cargo build --release --bin actl-signal` → `display-start`（本轮已重建并就绪）。
3. 隔离 LOCALAPPDATA 会连显示 lease 一起隔离——需要显示门的命令（launch 等）验收时必须用真实环境。
4. **关闭带未保存内容的 GUI 应用会弹保存框**：close-window 后用 front-report 读引导层（「是否要保存? 保存/不保存/取消」），按清理白名单点击「不保存」（find @ref + click + snapshot 绑定）完成退出——本轮即为该机制的设计用例闭环。
