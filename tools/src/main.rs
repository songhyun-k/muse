mod build;
mod process;

use process::{Result, ensure};
use std::{env, path::Path};

fn main() {
    if let Err(error) = execute() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn execute() -> Result {
    env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;
    let args: Vec<_> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("--help");
    let allowed: &[&str] = match command {
        "build" => &["--release"],
        "check-publication" | "--help" => &[],
        _ => return Err(format!("Unknown task: {command}").into()),
    };
    for flag in args.iter().skip(1) {
        ensure(
            allowed.contains(&flag.as_str()),
            &format!("Unknown option: {flag}"),
        )?;
    }
    match command {
        "build" => build::build(args.iter().any(|a| a == "--release")),
        "check-publication" => build::check_publication(),
        _ => {
            println!("cargo xtask build [--release]\ncargo xtask check-publication");
            Ok(())
        }
    }
}
