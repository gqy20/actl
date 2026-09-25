//! actl-cli —— 命令入口与分发(骨架)。
//!
//! M1 起基于 clap(derive)实现命令面;stdout 契约由 actl-core 的 envelope 提供(ADR-002)。

fn main() {
    // 骨架期输出走 stderr,避免与未来的 stdout JSON 契约混淆(ADR-002)。
    eprintln!(
        "actl v{} — scaffold; command surface arrives in M1 (docs/ROADMAP.md Phase 1)",
        env!("CARGO_PKG_VERSION")
    );
}
