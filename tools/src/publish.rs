use crate::{
    audit, bottle, build, package,
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

pub fn formula(version: &str, checksum: &str, bottle_checksum: Option<&str>) -> Result<String> {
    ensure(
        Regex::new(r"^[0-9a-f]{64}$")?.is_match(checksum),
        "Invalid archive checksum",
    )?;
    let bottle = if let Some(checksum) = bottle_checksum {
        ensure(
            Regex::new(r"^[0-9a-f]{64}$")?.is_match(checksum),
            "Invalid bottle checksum",
        )?;
        format!(
            "  bottle do\n    root_url \"https://github.com/{REPO}/releases/download/v{version}\"\n    sha256 cellar: :any_skip_relocation, arm64_sonoma: \"{checksum}\"\n  end\n"
        )
    } else {
        String::new()
    };
    Ok(include_str!("../../packaging/muse.rb.in")
        .replace("@BOTTLE@", &bottle)
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
    ensure(
        output(Command::new("git").args([
            "ls-remote",
            &format!("https://github.com/{REPO}.git"),
            &format!("refs/tags/v{version}"),
        ]))?
        .is_empty(),
        "Release tag already exists; publish a new version",
    )?;
    let report: Value = serde_json::from_slice(&fs::read("dist/release.json")?)?;
    verify_report(&report, version, &head)?;
    let bottle = Path::new("dist").join(bottle::filename(version));
    let bottle_checksum = package::digest(&bottle)?;
    verify_bottle_report(&report, version, &bottle_checksum)?;
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
        formula(version, &package::digest(&archive)?, Some(&bottle_checksum))?,
    )?;
    let mut assets = vec![
        archive,
        bottle,
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
    let bottle_asset = release["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == bottle::filename(version))
        .ok_or("Missing release bottle")?;
    let bottle_checksum = bottle_asset["digest"]
        .as_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .ok_or("Missing published bottle digest")?;
    let formula = formula(version, checksum, Some(bottle_checksum))?;
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

fn verify_bottle_report(report: &Value, version: &str, checksum: &str) -> Result {
    ensure(
        report["bottle"] == bottle::filename(version)
            && report["bottleSha256"] == checksum
            && report["bottleInstalledOnMacOS14"] == true,
        "Bottle changed or was not installed and verified on macOS 14",
    )
}

#[test]
fn publication_requires_the_exact_installed_bottle() -> Result {
    let checksum = "a".repeat(64);
    let mut report = json!({"bottle":bottle::filename("0.3.1"),"bottleSha256":checksum,"bottleInstalledOnMacOS14":true});
    verify_bottle_report(&report, "0.3.1", &checksum)?;
    assert!(verify_bottle_report(&report, "0.3.2", &checksum).is_err());
    assert!(verify_bottle_report(&report, "0.3.1", &"b".repeat(64)).is_err());
    report["bottleInstalledOnMacOS14"] = false.into();
    assert!(verify_bottle_report(&report, "0.3.1", &checksum).is_err());
    assert!(formula("0.3.1", &checksum, Some("invalid")).is_err());
    let formula = formula("0.3.1", &checksum, Some(&checksum))?;
    assert!(formula.contains("arm64_sonoma:") && !formula.contains("@BOTTLE@"));
    Ok(())
}
