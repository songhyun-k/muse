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
checksums, build manifest and a native Homebrew bottle. The `songhyun-k/tap` Homebrew tap selects an exact
release URL and SHA-256 for both the archive and bottle. The relocatable
`arm64_sonoma` bottle supports Apple Silicon macOS 14 and later, so ordinary
installation does not enter Homebrew's source-build toolchain checks. Homebrew upgrades replace the executable; application
preferences and playlists remain in the user's Application Support directory.

Run the complete offline gate before packaging. The release job must verify the
version, clean source commit, artifact hashes and relocated execution before it
publishes. The release runs on macOS 14 and uses `brew bottle` to generate
the native package and receipt. A fresh bottle installation must preserve the
inspected executable and pass its signature and demo probe checks. Installation
validation points only the disposable tap's bottle root at the current local
artifact directory; it does not seed a public bottle URL's download cache. The
packaged formula and publication metadata retain their public release URLs. Tap updates follow the published release and are checked by installing
and running the downloaded package, never by merely checking that the URL exists.

Production credentials and developer signing identities do not enter the repository.
The current packages use ad-hoc signing. Release automation uses repository-scoped
GitHub credentials; updating the separate tap uses the maintainer's GitHub login.

See [build and use](RELEASE.md), [validation](VALIDATION.md) and
[GitHub branch protection](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).

## Publish a version

Change `frontend/Cargo.toml`, refresh Cargo.lock and merge the passing change into main.
Run `gh workflow run release.yml --ref main`; the job checks, packages and publishes
the version from main. An existing version is never overwritten. Validation
rejects any existing version tag in `songhyun-k/muse`, even without a Release or
when the tag already points to the inspected commit; choose a new version.

The packaging sequence is `cargo xtask check`, `cargo xtask release`,
`cargo xtask bottle`, `cargo xtask audit --export`, then `cargo xtask publish --publish`.
The bottle task requires a disposable macOS 14 arm64 GitHub runner with no existing
muse tap or installation; it must not replace a developer's local Homebrew setup.
Local checks and standalone archive inspection remain available without Homebrew.
Without flags, `cargo xtask publish` only validates and creates the formula/checksum files.
After publication, `cargo xtask publish --update-tap` updates the exact
version/checksum in the tap through the maintainer's `gh` login.
