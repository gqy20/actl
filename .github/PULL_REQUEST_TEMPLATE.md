## 变更

(一句话说明做了什么、为什么)

## 检查清单

- [ ] `cargo xtask gate` 通过(与 pre-push / CI 同一实现)
- [ ] 若涉及协议/命令语义变更(`spec` 提交):已同步契约 golden + docs/06 + CHANGELOG(AGENTS.md 硬规则 8)
- [ ] 若新增依赖:已登记 docs/TECH_STACK.md §3,直接依赖总数仍 ≤ 8(硬规则 6)
- [ ] 若修 bug:复现测试与修复同 PR(AGENTS.md 测试节)

## 备注

(复现方式 / evidence 输出 / 截图,可选)
