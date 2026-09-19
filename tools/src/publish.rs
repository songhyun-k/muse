use crate::{
    audit, build, package,
    process::{Result, ensure, output, run},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const REPO: &str = "songhyun-k/muse";
const TAP: &str = "songhyun-k/homebrew-tap";

fn formula(version: &str, checksum: &str) -> Result<String> {
    ensure(
        Regex::new(r"^[0-9a-f]{64}$")?.is_match(checksum),
        "Invalid archive checksum",
    )?;
    Ok(fs::read_to_string("packaging/muse.rb.in")?
        .replace("@VERSION@", version)
        .replace("@SHA256@", checksum))
}

fn verify_report(report: &Value, version: &str, head: &str) -> Result {
    ensure(
        report["commit"] == head && report["dirty"] == false,
        "Rebuild this clean commit",
    )?;
    ensure(
        report["version"] == version && report["architecture"] == "arm64",
        "Release version or architecture mismatch",
    )?;
    ensure(
        report["relocatedLaunch"] == true && report["archive"] == "muse-macos-arm64.tar.gz",
        "Release was not verified for distribution",
    )
}

fn validate(version: &str) -> Result<(String, Vec<PathBuf>)> {
    ensure(
        output(Command::new("git").args(["status", "--porcelain"]))?.is_empty(),
        "Commit changes before publishing",
    )?;
    let head = output(Command::new("git").args(["rev-parse", "HEAD"]))?;
    let remote = output(Command::new("git").args(["ls-remote", "origin", "refs/heads/main"]))?;
    ensure(
        remote.split_whitespace().next() == Some(head.as_str()),
        "Publish current remote main only",
    )?;
    let report: Value = serde_json::from_slice(&fs::read("dist/release.json")?)?;
    verify_report(&report, version, &head)?;
    let archive = PathBuf::from("dist/muse-macos-arm64.tar.gz");
    let binary = Path::new("dist/muse");
    ensure(
        package::digest(&archive)? == report["archiveSha256"]
            && package::digest(binary)? == report["sha256"],
        "Release artifacts changed after inspection",
    )?;
    let entries: Vec<_> = [
        "dist/muse",
        "LICENSE",
        "NOTICE.md",
        "dist/THIRD_PARTY_LICENSES.txt",
    ]
    .iter()
    .map(|p| {
        (
            PathBuf::from(p),
            PathBuf::from(Path::new(p).file_name().unwrap()),
        )
    })
    .collect();
    package::verify_archive(&archive, &entries)?;
    let source = PathBuf::from("dist/muse-source.tar.gz");
    let entries: Vec<_> = audit::source_files()?
        .into_iter()
        .map(|p| (p.clone(), Path::new("muse").join(p)))
        .collect();
    package::verify_archive(&source, &entries)?;
    fs::write(
        "dist/muse.rb",
        formula(version, &package::digest(&archive)?)?,
    )?;
    let mut assets = vec![
        archive,
        source,
        PathBuf::from("dist/release.json"),
        PathBuf::from("dist/muse.rb"),
    ];
    let mut checksums = String::new();
    for asset in &assets {
        checksums.push_str(&format!(
            "{}  {}\n",
            package::digest(asset)?,
            asset.file_name().unwrap().to_string_lossy()
        ));
    }
    fs::write("dist/SHA256SUMS", checksums)?;
    assets.push(PathBuf::from("dist/SHA256SUMS"));
    Ok((head, assets))
}

fn update_tap(version: &str) -> Result {
    let release: Value = serde_json::from_str(&output(
        Command::new("gh").args(["api", &format!("repos/{REPO}/releases/tags/v{version}")]),
    )?)?;
    ensure(
        release["draft"] == false && release["prerelease"] == false && release["immutable"] == true,
        "Expected an immutable stable release",
    )?;
    let asset = release["assets"]
        .as_array()
        .ok_or("Missing assets")?
        .iter()
        .find(|a| a["name"] == "muse-macos-arm64.tar.gz")
        .ok_or("Missing release archive")?;
    let checksum = asset["digest"]
        .as_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .ok_or("Missing published digest")?;
    let formula = formula(version, checksum)?;
    let endpoint = format!("repos/{TAP}/contents/Formula/muse.rb");
    let current: Value =
        serde_json::from_str(&output(Command::new("gh").args(["api", &endpoint]))?)?;
    let content: String = current["content"]
        .as_str()
        .ok_or("Missing formula content")?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let previous = String::from_utf8(STANDARD.decode(content)?)?;
    if previous == formula {
        return Ok(());
    }
    let pattern = Regex::new(r"/download/v(\d+\.\d+\.\d+)/")?;
    let old = pattern
        .captures(&previous)
        .ok_or("Cannot identify installed tap version")?;
    let numbers = |v: &str| -> Result<Vec<u64>> {
        Ok(v.split('.')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?)
    };
    ensure(
        numbers(&old[1])? <= numbers(version)?,
        "Refusing to downgrade the tap",
    )?;
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(
        &mut file,
        &json!({"message":format!("Release muse {version}"),"sha":current["sha"],"content":STANDARD.encode(formula),"branch":"main"}),
    )?;
    run(Command::new("gh")
        .args(["api", "--method", "PUT", &endpoint, "--input"])
        .arg(file.path())
        .args(["--jq", ".commit.html_url"]))
}

pub fn publish(upload: bool, tap: bool) -> Result {
    let version = build::version()?;
    ensure(
        Regex::new(r"^\d+\.\d+\.\d+$")?.is_match(&version),
        "Expected a stable semantic version",
    )?;
    if tap && !upload {
        return update_tap(&version);
    }
    let (head, assets) = validate(&version)?;
    println!("Verified muse {version} at {head}");
    if upload {
        let mut notes = tempfile::NamedTempFile::new()?;
        notes.write_all(b"Apple Music in your terminal. macOS 14+ on Apple Silicon.\n\n```sh\nbrew install songhyun-k/tap/muse\nmuse\n```\n\nSign in to the Music app on your Mac. Press `,` for Settings or `?` for help.\n\nThe archive includes the executable and license notices. No extra runtime is required.\n`SHA256SUMS` and `release.json` identify the inspected artifacts. Packages use ad-hoc signing.\n")?;
        run(Command::new("gh")
            .args(["release", "create", &format!("v{version}")])
            .args(&assets)
            .args([
                "--repo",
                REPO,
                "--target",
                &head,
                "--title",
                &format!("muse {version}"),
                "--notes-file",
            ])
            .arg(notes.path())
            .arg("--latest"))?;
    }
    if tap {
        update_tap(&version)?;
    }
    Ok(())
}

#[test]
fn release_requires_the_inspected_version_and_clean_commit() -> Result {
    let mut report = json!({"commit":"head","dirty":false,"version":"1.0.0","architecture":"arm64","relocatedLaunch":true,"archive":"muse-macos-arm64.tar.gz"});
    verify_report(&report, "1.0.0", "head")?;
    assert!(verify_report(&report, "1.0.1", "head").is_err());
    assert!(verify_report(&report, "1.0.0", "new-head").is_err());
    report["dirty"] = json!(true);
    assert!(verify_report(&report, "1.0.0", "head").is_err());
    Ok(())
}
