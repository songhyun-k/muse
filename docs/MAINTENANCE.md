# Branches and releases

`main` is the integration and release branch. Use short `feat/<name>` or
`fix/<name>` branches, open a PR, pass CI and rebase-merge. Delete merged branches.
There is no permanent develop or release branch. Keep commits as small logical
units; rebase on current main before assigning the final checklist entries.
Unit IDs belong in planning documents; commit subjects have no required prefix.

Main requires the `check` CI job and linear history. Force pushes and branch
deletion are disabled. A solo maintainer may merge their own passing PR; an
additional reviewer is not required. Private development archives are never pushed.

## Versioning

The package version in `frontend/Cargo.toml` is the product version. Use semantic
versions: patch for fixes, minor for new features, major for incompatible changes.
Release tags are `v<version>` and identify a clean main commit. Published tags and
assets are immutable; fixes ship in a new version instead of replacing an archive.

## Distribution

GitHub Releases carry the Apple Silicon executable, license notices, source archive,
checksums and build manifest. The `songhyun-k/tap` Homebrew tap selects an exact
release URL and SHA-256. Homebrew upgrades replace the executable; application
preferences and playlists remain in the user's Application Support directory.

Run the complete offline gate before packaging. The release job must verify the
version, clean source commit, artifact hashes and relocated execution before it
publishes. Tap updates follow the published release and are checked by installing
and running the downloaded package, never by merely checking that the URL exists.

Production credentials and developer signing identities do not enter the repository.
The current packages use ad-hoc signing. Release automation uses repository-scoped
GitHub credentials; updating the separate tap uses the maintainer's GitHub login.

See [build and use](RELEASE.md), [validation](VALIDATION.md) and
[GitHub branch protection](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).

## Publish a version

Change `frontend/Cargo.toml`, refresh Cargo.lock and merge the passing change into main.
Run `gh workflow run release.yml --ref main`; the job checks, packages and publishes
the version from main. An existing version is never overwritten.

The local path is `cargo xtask check`, `cargo xtask release`,
`cargo xtask audit --export`, then `cargo xtask publish --publish`.
Without flags, `cargo xtask publish` only validates and creates the formula/checksum files.
After publication, `cargo xtask publish --update-tap` updates the exact
version/checksum in the tap through the maintainer's `gh` login.
