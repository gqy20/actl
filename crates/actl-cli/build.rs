fn main() {
    // clap's debug command-tree construction exceeds MSVC's default 1 MiB reserve.
    // Reserve 4 MiB; Windows commits pages on demand, including in release builds.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=actl=/STACK:4194304");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
