# crates.io 发布手册(当前已发布至 0.1.3:actl-core / actl-uia / actl-cli 同版本;
# `cargo install actl-cli` → bin `actl` 全链路验收通过)

**已自动化(2026-09-26 起)**:推送 vX.Y.Z tag 时 release.yml 的 crates job 自动按序发布三 crate
(token 已配为仓库 secret `CARGO_REGISTRY_TOKEN`,经 `gh secret set` 注入);
版本守卫要求 tag == Cargo.toml version,错位即 fail。手动路径仅作兜底:

`actl` 名已被 2018 年的旧包占用,使用 `actl-cli`(bin 名仍是 `actl`)。
按依赖顺序:

```bash
cargo login                                   # 一次性
cargo publish -p actl-core                    # ① 协议层(无本仓库依赖)
cargo publish -p actl-uia                     # ② UIA 后端(依赖 ①,上架后可解析)
cargo publish -p actl-cli                     # ③ 主程序(cargo install actl-cli → bin actl)
```

注:②③ 的 `--dry-run` 在 ①② 未上架时会报 "no matching package named actl-core/uia",
属预期;core 的 dry-run 已验证通过(v0.1.1,36.5KiB)。
scoop 与 GitHub Releases 是主分发渠道(GitHub Actions 自动);crates.io 为补充。

## crates.io 页面 README 渲染(规范)

crates.io 只渲染**打包进 crate 的 README 文件**(Cargo.toml `readme` 字段指定,
相对路径指向仓库根 README);没有该字段 = 页面空白。三 crate 均已配
`readme = "../../README.md"`,dry-run 验证 tarball 内含 `README.md`。
版本不可变:0.1.2 及之前发布到 crates.io 的版本页面无法补 README,
从下一版起生效。
