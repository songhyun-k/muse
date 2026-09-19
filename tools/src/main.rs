mod build;
mod demo;
mod generate;
mod process;
mod terminal;

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
    if command == "__pty-child" {
        ensure(args.len() == 2, "PTY child requires a binary")?;
        std::process::exit(terminal::child(Path::new(&args[1]))?);
    }
    let allowed: &[&str] = match command {
        "build" => &["--release"],
        "generate" | "demo" => &["--check"],
        "terminal" | "previews" | "check-publication" | "--help" => &[],
        _ => return Err(format!("Unknown task: {command}").into()),
    };
    for flag in args.iter().skip(1) {
        ensure(
            allowed.contains(&flag.as_str()),
            &format!("Unknown option: {flag}"),
        )?;
    }
    match command {
        "demo" => demo::generate(args.iter().any(|a| a == "--check")),
        "generate" => generate::generate(args.iter().any(|a| a == "--check")),
        "build" => build::build(args.iter().any(|a| a == "--release")),
        "check-publication" => build::check_publication(),
        "previews" => demo::previews(),
        "terminal" => terminal::check(Path::new("dist/muse")),
        _ => {
            println!("cargo xtask build [--release]\ncargo xtask check-publication");
            Ok(())
        }
    }
}
