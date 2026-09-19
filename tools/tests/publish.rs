use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn existing_tag_and_lookup_failure_block_publication() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    for (name, script) in [
        (
            "cargo",
            "#!/bin/sh\nprintf '%s\\n' '{\"packages\":[{\"version\":\"1.0.0\"}]}'\n",
        ),
        (
            "git",
            r#"#!/bin/sh
case "$*" in
    'status --porcelain') ;;
    'rev-parse HEAD') printf '%s\n' bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb ;;
    'ls-remote origin refs/heads/main')
        printf '%s\t%s\n' bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/main ;;
    'ls-remote https://github.com/songhyun-k/muse.git refs/tags/v1.0.0')
        if [ "$MUSE_TEST_TAG" = unavailable ]; then
            echo 'tag lookup unavailable' >&2
            exit 128
        fi
        printf '%s\t%s\n' "$MUSE_TEST_TAG" refs/tags/v1.0.0 ;;
    *) echo "Unexpected git arguments: $*" >&2; exit 1 ;;
esac
"#,
        ),
        (
            "gh",
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$MUSE_TEST_UPLOAD\"\nexit 1\n",
        ),
    ] {
        let path = temporary.path().join(name);
        fs::write(&path, script)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    let upload = temporary.path().join("upload");
    for tag in [
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "unavailable",
    ] {
        for flags in [vec![], vec!["--publish"], vec!["--publish", "--update-tap"]] {
            let result = Command::new(env!("CARGO_BIN_EXE_muse-tools"))
                .arg("publish")
                .args(flags)
                .env("PATH", temporary.path())
                .env("MUSE_TEST_TAG", tag)
                .env("MUSE_TEST_UPLOAD", &upload)
                .output()?;
            let error = String::from_utf8(result.stderr)?;
            assert!(!result.status.success());
            let expected = if tag == "unavailable" {
                "exit status: 128"
            } else {
                "Release tag already exists; publish a new version"
            };
            assert!(error.contains(expected), "{error}");
            assert!(
                !upload.exists(),
                "Publication attempted before rejecting tag"
            );
        }
    }
    Ok(())
}
