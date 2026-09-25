//! actl-physical —— 物理输入兜底(骨架)。
//!
//! 安全约束(ADR-007 / docs/PRD.md §4):本 crate 的一切能力仅可通过显式
//! `--physical` 路径触达;默认执行路径(UIA 后台调用)不得引用本 crate。
//! 分发形态:编译期 feature 可整体裁剪(docs/TECH_STACK.md §3)。

/// 物理后端标识,供 `status` 命令与诊断输出使用。
pub fn backend() -> &'static str {
    "physical-scaffold"
}

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_is_alive() {
        assert_eq!(super::backend(), "physical-scaffold");
    }
}
