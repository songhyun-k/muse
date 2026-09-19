use crate::{
    package,
    process::{Result, ensure, output},
};
use regex::bytes::Regex;
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn source_files() -> Result<Vec<PathBuf>> {
    let files = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .stderr(Stdio::inherit())
        .output()?;
    ensure(files.status.success(), "Git source listing failed")?;
    Ok(files
        .stdout
        .split(|byte| *byte == 0)
        .filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(OsStr::from_bytes(p)))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn patterns() -> Result<Vec<(&'static str, Regex)>> {
    [
        (
            "private key",
            r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
        ),
        ("GitHub token", r"gh[pousr]_[A-Za-z0-9]{30,}"),
        (
            "JWT",
            r"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}",
        ),
        ("personal absolute path", r"/Users/[A-Za-z0-9_.-]+/"),
    ]
    .into_iter()
    .map(|(kind, pattern)| Ok((kind, Regex::new(pattern)?)))
    .collect()
}

fn findings(bytes: &[u8], patterns: &[(&str, Regex)]) -> Vec<String> {
    patterns
        .iter()
        .filter(|(kind, pattern)| {
            pattern.find_iter(bytes).any(|m| {
                *kind != "personal absolute path"
                    || ![b"/Users/example/".as_slice(), b"/Users/yourname/"].contains(&m.as_bytes())
            })
        })
        .map(|(kind, _)| (*kind).to_owned())
        .collect()
}

fn history(patterns: &[(&str, Regex)]) -> Result {
    let objects = output(Command::new("git").args(["rev-list", "--objects", "--all"]))?;
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let checked = (|| -> Result<Vec<String>> {
        let mut errors = Vec::new();
        for row in objects.lines() {
            let (id, name) = row.split_once(' ').unwrap_or((row, ""));
            writeln!(input, "{id}")?;
            input.flush()?;
            let mut header = String::new();
            reader.read_line(&mut header)?;
            let fields: Vec<_> = header.split_whitespace().collect();
            ensure(fields.len() == 3, "Invalid Git object header")?;
            let size: usize = fields[2].parse()?;
            let mut bytes = vec![0; size];
            reader.read_exact(&mut bytes)?;
            reader.read_exact(&mut [0; 1])?;
            if fields[1] == "blob" {
                for kind in findings(&bytes, patterns) {
                    errors.push(format!("{name}: {kind}"));
                }
            }
        }
        Ok(errors)
    })();
    drop(input);
    if checked.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    ensure(status.success(), "Git history inspection failed")?;
    let errors = checked?;
    ensure(
        errors.is_empty(),
        &format!("Sensitive patterns in history (values withheld): {errors:?}"),
    )?;
    println!("Source history audit passed");
    Ok(())
}

pub fn audit(check_history: bool, export: bool) -> Result {
    let patterns = patterns()?;
    let files = source_files()?;
    for path in &files {
        let found = findings(&fs::read(path)?, &patterns);
        ensure(
            found.is_empty(),
            &format!(
                "{}: sensitive patterns (values withheld): {found:?}",
                path.display()
            ),
        )?;
    }
    if check_history {
        history(&patterns)?;
    }
    if export {
        fs::create_dir_all("dist")?;
        let entries: Vec<_> = files
            .iter()
            .map(|p| (p.clone(), Path::new("muse").join(p)))
            .collect();
        let destination = Path::new("dist/muse-source.tar.gz");
        package::archive(destination, &entries)?;
        package::verify_archive(destination, &entries)?;
        println!("Source archive SHA-256: {}", package::digest(destination)?);
    }
    println!("Source audit passed ({} files)", files.len());
    Ok(())
}

#[test]
fn source_paths_preserve_leading_whitespace() -> Result {
    use crate::process::run;

    // A child test process keeps the temporary working directory isolated.
    if std::env::var_os("MUSE_AUDIT_PATH_TEST").is_none() {
        let temporary = tempfile::tempdir()?;
        return run(Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "audit::source_paths_preserve_leading_whitespace",
                "--nocapture",
            ])
            .env("MUSE_AUDIT_PATH_TEST", "1")
            .current_dir(temporary.path()));
    }

    run(Command::new("git").args(["init", "--quiet"]))?;
    fs::write(".git/info/exclude", "/dist/\n")?;
    let name = " leading.txt";
    fs::write(name, "safe source\n")?;
    run(Command::new("git").args(["add", "--", name]))?;
    assert_eq!(source_files()?, [PathBuf::from(name)]);
    audit(false, true)?;
    package::verify_archive(
        Path::new("dist/muse-source.tar.gz"),
        &[(PathBuf::from(name), Path::new("muse").join(name))],
    )?;

    let token = format!("ghp_{}", "a".repeat(36));
    fs::write(name, &token)?;
    let error = audit(false, true).unwrap_err().to_string();
    assert!(error.contains("GitHub token"));
    assert!(!error.contains(&token));
    run(Command::new("git").args(["mv", "--", name, "ordinary.txt"]))?;
    let renamed_error = audit(false, true).unwrap_err().to_string();
    assert_eq!(
        error.strip_prefix(name).unwrap(),
        renamed_error.strip_prefix("ordinary.txt").unwrap()
    );
    fs::remove_file("ordinary.txt")?;
    assert!(audit(false, true).is_err());
    Ok(())
}

#[test]
fn secrets_are_detected_without_returning_their_values() -> Result {
    let patterns = patterns()?;
    let token = format!("ghp_{}", "a".repeat(36));
    assert_eq!(findings(token.as_bytes(), &patterns), ["GitHub token"]);
    assert!(findings(b"/Users/example/source /Users/yourname/source", &patterns).is_empty());
    let path = ["/Users/", "private-account/source"].concat();
    assert_eq!(
        findings(path.as_bytes(), &patterns),
        ["personal absolute path"]
    );
    Ok(())
}
