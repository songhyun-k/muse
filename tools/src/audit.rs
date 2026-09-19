use crate::{
    package,
    process::{Result, ensure, output},
};
use regex::bytes::Regex;
use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn source_files() -> Result<Vec<PathBuf>> {
    let files = output(Command::new("git").args([
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
    ]))?;
    Ok(files
        .split('\0')
        .filter(|p| !p.is_empty() && Path::new(p).is_file())
        .map(PathBuf::from)
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
