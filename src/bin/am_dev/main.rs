//! Developer-only tooling; never shipped. Built with `--features dev-tools`.
#[allow(dead_code)]
mod obligations;

use std::path::Path;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let code = match arguments.split_first() {
        Some((command, rest)) if command == "obligations" => {
            obligations::run(rest, Path::new(env!("CARGO_MANIFEST_DIR")))
        }
        _ => {
            eprintln!("usage: am-dev obligations summary|mark ...");
            2
        }
    };
    std::process::exit(code);
}
