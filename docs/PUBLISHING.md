# crates.io 发布手册(v0.1.1 已发布:actl-core / actl-uia / actl-cli 均 0.1.1;
# `cargo install actl-cli` → bin `actl` 全链路验收通过)

下次发版(版本 bump 后按序重跑;前置:token 已在 CARGO_HOME/credentials.toml):

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
