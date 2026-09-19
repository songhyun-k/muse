use crate::process::{Result, ensure, filter, output, run, timed};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    env, fs,
    path::Path,
    process::{Command, Stdio},
};

pub fn version() -> Result<String> {
    let metadata: Value = serde_json::from_str(&output(Command::new("cargo").args([
        "metadata",
        "--locked",
        "--no-deps",
        "--format-version",
        "1",
        "--manifest-path",
        "frontend/Cargo.toml",
    ]))?)?;
    Ok(metadata["packages"][0]["version"]
        .as_str()
        .ok_or("Missing product version")?
        .into())
}

pub fn publish(
    binary: &Path,
    destination: &Path,
    bundle: &str,
    identity: &str,
    strip: bool,
) -> Result {
    let parent = destination
        .parent()
        .ok_or("Missing destination directory")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".muse-")
        .tempdir_in(parent)?;
    let staged = temporary.path().join("muse");
    fs::copy(binary, &staged)?;
    if strip {
        run(Command::new("strip").arg("-S").arg(&staged))?;
    }
    let mut sign = Command::new("codesign");
    sign.args(["--force", "--sign", identity, "--identifier", bundle]);
    if identity != "-" {
        sign.args(["--options", "runtime", "--timestamp"]);
    }
    run(sign.arg(&staged))?;
    run(Command::new("codesign")
        .args(["--verify", "--strict"])
        .arg(&staged))?;
    timed(Command::new(&staged).arg("--probe"), 10)?;
    fs::rename(staged, destination)?;
    Ok(())
}

pub fn build(release: bool) -> Result {
    let profile = if release { "release" } else { "debug" };
    let bundle = env::var("MUSIC_BUNDLE_ID").unwrap_or("local.muse.cli".into());
    ensure(
        Regex::new(r"^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$")?.is_match(&bundle),
        "Invalid MUSIC_BUNDLE_ID",
    )?;
    let info = json!({
        "CFBundleName":"muse", "CFBundleIdentifier":bundle, "CFBundleShortVersionString":version()?,
        "NSAppleMusicUsageDescription":"Browse your music library and play Apple Music. 음악 보관함을 탐색하고 음악을 재생합니다."
    });
    fs::create_dir_all("backend/.build")?;
    let encoded = filter(
        Command::new("plutil").args(["-convert", "xml1", "-o", "-", "--", "-"]),
        &serde_json::to_vec(&info)?,
    )?;
    let plist = Path::new("backend/.build/Info.plist");
    if fs::read(plist).ok().as_deref() != Some(&encoded) {
        fs::write(plist, encoded)?;
    }
    let mut cargo = Command::new("cargo");
    cargo.args([
        "build",
        "--locked",
        "--manifest-path",
        "frontend/Cargo.toml",
    ]);
    let mut swift = Command::new("swift");
    swift
        .args(["build", "--package-path", "backend", "-c", profile])
        .env("MUSIC_BUILD_PROFILE", profile);
    if release {
        cargo.arg("--release");
        let root = env::current_dir()?;
        let cache = env::var_os("CARGO_HOME")
            .map(Into::into)
            .unwrap_or(Path::new(&env::var("HOME").unwrap_or_default()).join(".cargo"));
        let mut flags: Vec<String> = match env::var("CARGO_ENCODED_RUSTFLAGS") {
            Ok(flags) => flags
                .split('\x1f')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
            Err(_) => shlex::split(&env::var("RUSTFLAGS").unwrap_or_default())
                .ok_or("Invalid RUSTFLAGS quoting")?,
        };
        flags.extend([
            format!("--remap-path-prefix={}=muse", root.display()),
            format!(
                "--remap-path-prefix={}=cargo",
                cache.canonicalize()?.display()
            ),
        ]);
        cargo.env("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"));
        swift.args([
            "-Xswiftc",
            "-gnone",
            "-Xswiftc",
            "-file-prefix-map",
            "-Xswiftc",
            &format!("{}=/muse", root.display()),
        ]);
    }
    let bin = output(swift.arg("--show-bin-path"))?;
    // Drop only --show-bin-path; retain the same profile, flags and environment.
    let args: Vec<_> = swift.get_args().map(|a| a.to_os_string()).collect();
    let mut compile = Command::new("swift");
    compile
        .args(&args[..args.len() - 1])
        .env("MUSIC_BUILD_PROFILE", profile);
    run(&mut cargo)?;
    let binary = Path::new(&bin).join("muse");
    // SwiftPM does not track the external Rust archive or embedded plist: force relinking.
    if binary.exists() {
        fs::remove_file(&binary)?;
    }
    run(&mut compile)?;
    publish(
        &binary,
        Path::new("dist/muse"),
        &bundle,
        &env::var("MUSIC_SIGN_IDENTITY").unwrap_or("-".into()),
        release,
    )?;
    println!("dist/muse");
    Ok(())
}

pub fn check_publication() -> Result {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, PermissionsExt},
    };
    let temporary = tempfile::tempdir()?;
    let destination = temporary.path().join("muse");
    let source = Path::new("dist/muse");
    fs::copy(source, &destination)?;
    let original = fs::read(&destination)?;
    let inode = fs::metadata(&destination)?.ino();
    let mut open = fs::File::open(&destination)?;
    publish(source, &destination, "local.muse.check", "-", false)?;
    let mut old = Vec::new();
    open.read_to_end(&mut old)?;
    ensure(
        old == original && fs::metadata(&destination)?.ino() != inode,
        "Publication replaced the running inode",
    )?;
    let published = fs::read(&destination)?;
    let published_inode = fs::metadata(&destination)?.ino();
    let invalid = temporary.path().join("invalid");
    fs::write(&invalid, "#!/bin/sh\nexit 23\n")?;
    fs::set_permissions(&invalid, fs::Permissions::from_mode(0o755))?;
    ensure(
        publish(&invalid, &destination, "local.muse.check", "-", false).is_err(),
        "Invalid executable was published",
    )?;
    ensure(
        fs::metadata(&destination)?.ino() == published_inode
            && fs::read(&destination)? == published,
        "Failed publication lost the old executable",
    )?;
    timed(
        Command::new(&destination)
            .arg("--version")
            .stdout(Stdio::null()),
        10,
    )?;
    println!("Publication preserves running inodes and rejects invalid replacements");
    Ok(())
}
