use crate::{
    audit, build, demo, generate,
    process::{Result, run},
    terminal,
};
use std::{path::Path, process::Command};

pub fn check() -> Result {
    audit::audit(false, false)?;
    generate::generate(true)?;
    demo::generate(true)?;
    run(Command::new("git").args(["diff", "--check"]))?;
    build::build(false)?;
    run(Command::new("swift").args(["test", "--package-path", "backend"]))?;
    for manifest in ["frontend/Cargo.toml", "tools/Cargo.toml"] {
        for (command, args) in [
            ("fmt", vec!["--check"]),
            ("test", vec!["--locked"]),
            (
                "clippy",
                vec!["--locked", "--all-targets", "--", "-D", "warnings"],
            ),
        ] {
            run(Command::new("cargo")
                .args([command, "--manifest-path", manifest])
                .args(args))?;
        }
    }
    build::check_publication()?;
    terminal::check(Path::new("dist/muse"))?;
    println!("Build, contracts, behavior, publication and terminal checks passed");
    Ok(())
}
