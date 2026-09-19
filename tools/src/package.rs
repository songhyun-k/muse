use crate::{
    build,
    process::{Result, ensure, filter, output, run, timed},
    terminal,
};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

pub fn archive(destination: &Path, files: &[(PathBuf, PathBuf)]) -> Result {
    let mut archive = tar::Builder::new(GzEncoder::new(
        File::create(destination)?,
        Compression::default(),
    ));
    for (source, name) in files {
        ensure(
            fs::symlink_metadata(source)?.is_file(),
            "Archive inputs must be regular files",
        )?;
        let mut file = File::open(source)?;
        let metadata = file.metadata()?;
        let mut header = tar::Header::new_gnu();
        header.set_size(metadata.len());
        header.set_mode(metadata.mode() & 0o777);
        header.set_mtime(metadata.mtime().max(0) as u64);
        header.set_uid(0);
        header.set_gid(0);
        header.set_entry_type(tar::EntryType::Regular);
        archive.append_data(&mut header, name, &mut file)?;
    }
    archive.into_inner()?.finish()?;
    Ok(())
}

pub fn verify_archive(path: &Path, files: &[(PathBuf, PathBuf)]) -> Result {
    let expected: BTreeMap<_, _> = files
        .iter()
        .map(|(source, name)| (name.clone(), source))
        .collect();
    let mut seen = BTreeSet::new();
    for entry in tar::Archive::new(GzDecoder::new(File::open(path)?)).entries()? {
        let mut entry = entry?;
        let name = entry.path()?.into_owned();
        ensure(
            entry.header().entry_type().is_file() && seen.insert(name.clone()),
            "Invalid or duplicate archive entry",
        )?;
        let source = expected.get(&name).ok_or("Unexpected archive entry")?;
        ensure(
            entry.size() == fs::metadata(source)?.len(),
            "Archive size changed",
        )?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        ensure(bytes == fs::read(source)?, "Archive contents are stale")?;
    }
    ensure(seen.len() == expected.len(), "Archive is incomplete")
}

fn licenses() -> Result<PathBuf> {
    let rust = output(Command::new("rustc").arg("-vV"))?;
    let host = rust
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("Missing Rust host")?;
    let metadata: Value = serde_json::from_str(&output(Command::new("cargo").args([
        "metadata",
        "--locked",
        "--format-version",
        "1",
        "--filter-platform",
        host,
        "--manifest-path",
        "frontend/Cargo.toml",
    ]))?)?;
    let nodes: BTreeMap<_, _> = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("Missing dependency graph")?
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n))
        .collect();
    let root = metadata["resolve"]["root"]
        .as_str()
        .ok_or("Missing product root")?;
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if seen.insert(id) {
            pending.extend(
                nodes[id]["deps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| d["pkg"].as_str().unwrap()),
            );
        }
    }
    let mut packages: Vec<_> = metadata["packages"].as_array().unwrap().iter().collect();
    packages.sort_by_key(|p| (p["name"].as_str(), p["version"].as_str()));
    let mut notices = "Rust dependency notices for muse\n".to_owned();
    for package in packages {
        let id = package["id"].as_str().unwrap();
        if id == root || !seen.contains(id) {
            continue;
        }
        let directory = Path::new(package["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        let mut paths = Vec::new();
        for path in fs::read_dir(directory)? {
            let path = path?.path();
            let name = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_uppercase();
            if path.is_file()
                && ["LICENSE", "LICENCE", "COPYING", "NOTICE"]
                    .iter()
                    .any(|p| name.starts_with(p))
            {
                paths.push(path);
            }
        }
        paths.sort();
        ensure(
            !paths.is_empty(),
            &format!("Missing license text: {}", package["name"]),
        )?;
        notices.push_str(&format!(
            "\n## {} {} ({})\n",
            package["name"].as_str().unwrap(),
            package["version"].as_str().unwrap(),
            package["license"].as_str().unwrap_or("unspecified")
        ));
        for path in paths {
            notices.push_str(&format!(
                "\n{}\n\n{}\n",
                path.file_name().unwrap().to_string_lossy(),
                fs::read_to_string(&path)?
            ));
        }
    }
    let destination = PathBuf::from("dist/THIRD_PARTY_LICENSES.txt");
    fs::write(&destination, notices)?;
    Ok(destination)
}

pub fn embedded_metadata(binary: &Path, commands: &str) -> Result<Value> {
    let pattern = Regex::new(
        r"sectname __info_plist\s+segname __TEXT\s+addr \S+\s+size (0x[0-9a-fA-F]+)\s+offset (\d+)",
    )?;
    let captures = pattern
        .captures(commands)
        .ok_or("Missing embedded Info.plist")?;
    let size = usize::from_str_radix(&captures[1][2..], 16)?;
    let offset: usize = captures[2].parse()?;
    let data = fs::read(binary)?;
    let end = offset.checked_add(size).ok_or("Invalid plist size")?;
    let bytes = data.get(offset..end).ok_or("Invalid plist bounds")?;
    Ok(serde_json::from_slice(&filter(
        Command::new("plutil").args(["-convert", "json", "-o", "-", "--", "-"]),
        bytes,
    )?)?)
}

pub fn release() -> Result {
    build::build(true)?;
    let binary = Path::new("dist/muse");
    let libraries = output(Command::new("otool").arg("-L").arg(binary))?;
    let libraries: Vec<_> = libraries
        .lines()
        .skip(1)
        .map(|l| l.trim().split(" (").next().unwrap().to_owned())
        .collect();
    ensure(
        !libraries.is_empty()
            && libraries
                .iter()
                .all(|p| p.starts_with("/usr/lib/") || p.starts_with("/System/Library/")),
        "Executable depends on a non-system library",
    )?;
    let commands = output(Command::new("otool").arg("-l").arg(binary))?;
    let metadata = embedded_metadata(binary, &commands)?;
    let version = build::version()?;
    let bundle = std::env::var("MUSIC_BUNDLE_ID").unwrap_or("local.muse.cli".into());
    ensure(
        metadata["CFBundleIdentifier"] == bundle
            && metadata["CFBundleShortVersionString"] == version
            && metadata["NSAppleMusicUsageDescription"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
        "Invalid embedded metadata",
    )?;
    ensure(
        output(Command::new(binary).arg("--version"))?.starts_with(&format!("muse {version} ")),
        "Executable version mismatch",
    )?;
    ensure(
        !regex::bytes::Regex::new(r"/Users/[^/\x00\s]+/")?.is_match(&fs::read(binary)?),
        "Executable contains personal build paths",
    )?;
    run(Command::new("codesign")
        .args(["--verify", "--strict"])
        .arg(binary))?;
    let signature = Command::new("codesign")
        .args(["-dv", "--verbose=2"])
        .arg(binary)
        .output()?;
    ensure(signature.status.success(), "Cannot inspect signature")?;
    let signature = String::from_utf8(signature.stderr)?;
    ensure(
        signature
            .lines()
            .any(|l| l == format!("Identifier={bundle}")),
        "Signature identifier mismatch",
    )?;
    let temporary = tempfile::tempdir()?;
    let copied = temporary.path().join("muse");
    fs::copy(binary, &copied)?;
    for args in [&["--version"][..], &["--demo", "--probe"], &["--help"]] {
        timed(
            Command::new(&copied)
                .args(args)
                .current_dir(temporary.path())
                .env("PATH", "/usr/bin:/bin")
                .stdout(Stdio::null()),
            10,
        )?;
    }
    terminal::check(&copied)?;
    let architecture = output(Command::new("lipo").arg("-archs").arg(binary))?;
    let archive_path = PathBuf::from(format!(
        "dist/muse-macos-{}.tar.gz",
        architecture.replace(' ', "-")
    ));
    let files: Vec<_> = [
        binary.to_path_buf(),
        PathBuf::from("LICENSE"),
        PathBuf::from("NOTICE.md"),
        licenses()?,
    ]
    .into_iter()
    .map(|path| {
        let name = PathBuf::from(path.file_name().unwrap());
        (path, name)
    })
    .collect();
    archive(&archive_path, &files)?;
    verify_archive(&archive_path, &files)?;
    let minimum = Regex::new(r"(?s)cmd LC_BUILD_VERSION.*?\bminos (\S+)")?
        .captures(&commands)
        .ok_or("Missing minimum macOS version")?[1]
        .to_owned();
    let report = json!({"version":version,"commit":output(Command::new("git").args(["rev-parse","HEAD"]))?,"dirty":!output(Command::new("git").args(["status","--porcelain"]))?.is_empty(),
        "architecture":architecture,"minimumMacOS":minimum,"bundleIdentifier":bundle,"signing":if signature.contains("Signature=adhoc") {"ad-hoc"} else {"identity"},
        "sha256":digest(binary)?,"bytes":fs::metadata(binary)?.len(),"archive":archive_path.file_name().unwrap().to_string_lossy(),"archiveSha256":digest(&archive_path)?,"dynamicLibraries":libraries,"relocatedLaunch":true});
    fs::write(
        "dist/release.json",
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[test]
fn archive_rejects_stale_content_and_missing_members() -> Result {
    let temporary = tempfile::tempdir()?;
    let file = temporary.path().join("source");
    fs::write(&file, b"version one")?;
    let files = vec![(file.clone(), PathBuf::from("muse/source"))];
    let package = temporary.path().join("source.tar.gz");
    archive(&package, &files)?;
    verify_archive(&package, &files)?;
    fs::write(&file, b"version two")?;
    assert!(verify_archive(&package, &files).is_err());
    fs::write(&file, b"version one")?;
    let mut incomplete = files.clone();
    incomplete.push((file, PathBuf::from("muse/missing")));
    assert!(verify_archive(&package, &incomplete).is_err());
    Ok(())
}
