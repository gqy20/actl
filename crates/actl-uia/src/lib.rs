//! actl-uia —— Windows UIA 后端(骨架)。
//!
//! 本 crate 是全仓库唯一允许接触 COM/unsafe 的地方(AGENTS.md §5.5):
//! 引入第一个 unsafe 块时,在文件顶部加 `#![allow(unsafe_code)]`(workspace 级为
//! `deny`,此处保留唯一豁免点),并在本注释下登记豁免理由。
//!
//! 职责(docs/TECH_STACK.md §2):树遍历、可交互判定、pattern 分发(Invoke/Value/
//! Selection/…)、fallback 定位链。UIA 长尾脏活只允许出现在本 crate 内。

/// 后端标识,供 `status` 命令与诊断输出使用。
pub fn backend() -> &'static str {
    "uia-scaffold"
}

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_is_alive() {
        assert_eq!(super::backend(), "uia-scaffold");
    }
}
