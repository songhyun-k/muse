mod audit;
mod build;
mod check;
mod demo;
mod generate;
mod package;
mod process;
mod publish;
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
        "publish" => &["--publish", "--update-tap"],
        "audit" => &["--history", "--export"],
        "generate" | "demo" => &["--check"],
        "check" | "release" | "terminal" | "previews" | "check-publication" | "--help" => &[],
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
        "release" => package::release(),
        "check" => check::check(),
        "publish" => publish::publish(
            args.iter().any(|a| a == "--publish"),
            args.iter().any(|a| a == "--update-tap"),
        ),
        "audit" => audit::audit(
            args.iter().any(|a| a == "--history"),
            args.iter().any(|a| a == "--export"),
        ),
        "terminal" => terminal::check(Path::new("dist/muse")),
        _ => {
            println!(
                "cargo xtask build [--release]\ncargo xtask check\ncargo xtask generate [--check]\ncargo xtask demo [--check]\ncargo xtask previews\ncargo xtask release\ncargo xtask audit [--history] [--export]\ncargo xtask publish [--publish] [--update-tap]\ncargo xtask terminal\ncargo xtask check-publication"
            );
            Ok(())
        }
    }
}
