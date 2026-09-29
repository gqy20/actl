# 真实应用场景

业务步骤维护在 `skills/actl/workflows/`，直接使用 actl flow-run/flow-resume。
开发采样与真实应用夹具使用 `cargo xtask diagnose`、`cargo xtask fixture`。
不再维护 PS/Bash 启动包装；入口和迁移记录见 [原生迁移](../docs/61-native-tooling-migration.md)。
历史实测记录保留在 docs，新的 Rust 夹具和 workflow 需要独立验收。
