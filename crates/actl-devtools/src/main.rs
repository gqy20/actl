use actl_core::{CtlError, Envelope, ErrorEnvelope};
use actl_devtools::{Args, Result};
use serde_json::Value;
fn execute(args: &[String]) -> Result<Value> {
    match args.first().map(String::as_str) {
        Some("package") => actl_devtools::package::run(&Args::parse(&args[1..])?),
        Some("regress") => actl_devtools::regress::run(&Args::parse(&args[1..])?),
        Some("fixture" | "diagnose") => {
            let kind = args
                .get(1)
                .ok_or_else(|| CtlError::protocol("missing case"))?;
            let options = Args::parse(&args[2..])?;
            if args[0] == "fixture" {
                actl_devtools::fixtures::run(kind, &options)
            } else {
                actl_devtools::diagnostics::run(kind, &options)
            }
        }
        _ => Err(CtlError::protocol(
            "expected package, regress, fixture CASE, or diagnose KIND",
        )),
    }
}
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--help") {
        println!(
            "Usage: cargo xtask fixture CASE | diagnose KIND | regress | package [OPTIONS]\nOptions: --actl PATH --output NEW_DIRECTORY; regress --list --suite SUITE --case ID --unattended\nExample: cargo xtask regress --list\nErrors: PROTOCOL invalid arguments; ASSERTION_FAILED failed verification; INTERNAL capture failure"
        );
        return std::process::ExitCode::SUCCESS;
    }
    let start = std::time::Instant::now();
    let result = execute(&args);
    let ms = start.elapsed().as_millis() as u64;
    let (value, code) = match result {
        Ok(data) => {
            let code = if data["status"] == "blocked" {
                77
            } else if data["passed"] == false {
                1
            } else {
                0
            };
            (
                serde_json::to_string(&Envelope::success("devtools", Some(data), ms)),
                code,
            )
        }
        Err(error) => (
            serde_json::to_string(&ErrorEnvelope::new("devtools", &error, ms)),
            1,
        ),
    };
    match value {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("serialization failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    }
    std::process::ExitCode::from(code)
}
