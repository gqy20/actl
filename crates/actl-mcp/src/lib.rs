//! actl-mcp:远程访问被控设备的 MCP 适配服务(docs/82)。
//!
//! 分工铁律:**协议面归 rmcp,资产面归本 crate**——每次 `tools/call` spawn
//! 一个真实的 `actl <命令>` 子进程(与信号条 spawn record-start 同模式):
//! 预告、开始/继续本地放行、急停、看门狗、输入锁全部在被控端本地原样生效,
//! 远端结构性无法绕过任何一环。本 crate 不依赖任何 actl workspace crate
//! (纯薄壳,协议边界由编译器强制——ADR-001 修订的定界即此)。
//!
//! async/tokio 仅允许存在于本 crate(ADR-001 2026-10-05 修订:桌面命令
//! 路径保持同步,边界适配 crate 除外;见 TECH_STACK §4)。

pub mod dispatch;

/// 危险动词排除表(docs/82 §5.1):操作本地监督与恢复机制本身的动词出局;
/// 用户可见副作用(点窗/打字/剪贴板)不出局——它们是信号条预告的管辖对象。
/// 排除不是静默的:远端调用排除动词返回明确错误(本地专属+原因)。
/// 与速查描述(dispatch::QUICKREF)及资源模板(help 资源)三者由
/// 契约测试钉住一致。
pub const EXCLUDED_COMMANDS: &[&str] = &[
    // 停止/任务状态修复——急停代次只能本地清,这条纪律的结构性延伸。
    "execution-recover",
    // 显示端生命周期归本地人(显示端死了本地人重启)。
    "display-start",
    // 桌面窗口布局恢复(初版保守排除;有恢复记录护栏兜底,实现轮可重议)。
    "desktop-restore",
    // record-start 派生的内部进程缝,非用户动词——远端录制走 start/stop。
    "record-worker",
];

pub fn is_excluded(command: &str) -> bool {
    EXCLUDED_COMMANDS.contains(&command)
}
