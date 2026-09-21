use crate::{
    build, package,
    process::{Result, ensure, output, run},
    publish,
};
use serde_json::Value;
use std::{fs, os::unix::ffi::OsStrExt, path::Path, process::Command};

pub fn filename(version: &str) -> String {
    format!("muse-{version}.arm64_sonoma.bottle.tar.gz")
}

fn install_formula(version: &str, archive: &str, bottle: &str, directory: &Path) -> Result<String> {
    let public = publish::formula(version, archive, Some(bottle))?;
    let root =
        format!("root_url \"https://github.com/songhyun-k/muse/releases/download/v{version}\"");
    ensure(
        public.matches(&root).count() == 1,
        "Missing public bottle root",
    )?;
    let mut local = String::from("file://");
    for &byte in directory.canonicalize()?.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            local.push(byte as char);
        } else {
            local.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(public.replace(&root, &format!("root_url \"{local}\"")))
}

#[test]
fn install_uses_local_bottle_without_changing_public_formula() -> Result {
    let directory = tempfile::Builder::new().prefix("muse #한 ").tempdir()?;
    let archive = "a".repeat(64);
    let bottle = "b".repeat(64);
    let formula = install_formula("0.3.1", &archive, &bottle, directory.path())?;
    assert!(formula.contains("root_url \"file://"));
    assert!(formula.contains("%20%23%ED%95%9C%20"));
    assert!(formula.contains(&format!("arm64_sonoma: \"{bottle}\"")));
    assert!(formula.contains("url \"https://github.com/songhyun-k/muse/releases/download/v0.3.1/muse-macos-arm64.tar.gz\""));
    assert!(publish::formula("0.3.1", &archive, Some(&bottle))?.contains("root_url \"https://"));
    Ok(())
}

// Homebrew owns the bottle format and receipt. Only run on a disposable builder.
pub fn build() -> Result {
    ensure(
        std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
            && output(Command::new("sw_vers").arg("-productVersion"))?.starts_with("14.")
            && output(Command::new("uname").arg("-m"))? == "arm64",
        "Bottle creation requires a disposable macOS 14 arm64 GitHub runner",
    )?;
    let version = build::version()?;
    let mut report: Value = serde_json::from_slice(&fs::read("dist/release.json")?)?;
    ensure(
        report["version"] == version && report["minimumMacOS"] == "14.0",
        "Bottle must contain the inspected macOS 14 release",
    )?;
    let tap = output(Command::new("brew").args(["--repo", "songhyun-k/tap"]))?;
    let cellar = output(Command::new("brew").arg("--cellar"))?;
    ensure(
        !Path::new(&tap).exists() && !Path::new(&cellar).join("muse").exists(),
        "Bottle builder must not contain an existing muse installation or tap",
    )?;
    run(Command::new("brew").args(["tap-new", "songhyun-k/tap"]))?;
    fs::write(
        Path::new(&tap).join("Formula/muse.rb"),
        publish::formula(
            &version,
            &package::digest(Path::new("dist/muse-macos-arm64.tar.gz"))?,
            None,
        )?,
    )?;
    // Homebrew verifies this exact inspected source archive against the formula checksum.
    let cache = output(Command::new("brew").args(["--cache", "songhyun-k/tap/muse"]))?;
    fs::create_dir_all(
        Path::new(&cache)
            .parent()
            .ok_or("Missing cache directory")?,
    )?;
    fs::copy("dist/muse-macos-arm64.tar.gz", cache)?;
    run(Command::new("brew").args(["install", "--build-bottle", "songhyun-k/tap/muse"]))?;
    run(Command::new("brew")
        .args(["bottle", "--json", "--no-rebuild", "--root-url"])
        .arg(format!(
            "https://github.com/songhyun-k/muse/releases/download/v{version}"
        ))
        .arg("songhyun-k/tap/muse")
        .current_dir("dist"))?;
    let metadata: Value = serde_json::from_slice(&fs::read(format!(
        "dist/muse--{version}.arm64_sonoma.bottle.json"
    ))?)?;
    let bottle = &metadata["songhyun-k/tap/muse"]["bottle"];
    ensure(
        bottle["cellar"] == "any_skip_relocation",
        "Bottle must not require relocation",
    )?;
    let destination = Path::new("dist").join(filename(&version));
    fs::rename(
        format!("dist/muse--{version}.arm64_sonoma.bottle.tar.gz"),
        &destination,
    )?;
    let checksum = package::digest(&destination)?;
    ensure(
        bottle["tags"]["arm64_sonoma"]["sha256"] == checksum,
        "Homebrew bottle checksum mismatch",
    )?;
    fs::write(
        Path::new(&tap).join("Formula/muse.rb"),
        install_formula(
            &version,
            report["archiveSha256"]
                .as_str()
                .ok_or("Missing archive digest")?,
            &checksum,
            Path::new("dist"),
        )?,
    )?;
    // Test automatic bottle selection using this run's file, even if this version is already public.
    // The bottle was built above from the unchanged public formula; only the disposable tap changes.
    run(Command::new("brew").args(["uninstall", "songhyun-k/tap/muse"]))?;
    run(Command::new("brew")
        .args(["install", "songhyun-k/tap/muse"])
        .env("DEVELOPER_DIR", "/muse-no-developer-tools"))?;
    let installed = Path::new(&cellar).join("muse").join(&version);
    let receipt: Value =
        serde_json::from_slice(&fs::read(installed.join("INSTALL_RECEIPT.json"))?)?;
    ensure(
        receipt["poured_from_bottle"] == true,
        "Homebrew did not pour the bottle",
    )?;
    ensure(
        package::digest(&installed.join("bin/muse"))? == report["sha256"],
        "Homebrew changed the inspected executable",
    )?;
    run(Command::new(installed.join("bin/muse")).args(["--demo", "--probe"]))?;
    run(Command::new("codesign")
        .args(["--verify", "--strict"])
        .arg(installed.join("bin/muse")))?;
    report["bottle"] = filename(&version).into();
    report["bottleSha256"] = checksum.into();
    report["bottleInstalledOnMacOS14"] = true.into();
    fs::write(
        "dist/release.json",
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(())
}
