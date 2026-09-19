# Contributing

muse is a macOS app with a Swift service and a Rust/Ratatui frontend. Keep changes
small and focused. Playback, providers and domain persistence belong to Swift;
view state, localization, preferences and terminal rendering belong to Rust.
The versioned interface is [contract/api.json](contract/api.json).

## Build and verify

Install Xcode with Swift 6 and Rust 1.88+. From a checkout:

```sh
cargo xtask build
swift test --package-path backend
cargo test --locked --manifest-path frontend/Cargo.toml
cargo fmt --manifest-path frontend/Cargo.toml --check
cargo clippy --locked --manifest-path frontend/Cargo.toml --all-targets -- -D warnings
cargo xtask check
./dist/muse --demo --language en
```

`cargo xtask check` runs the shared offline checks used by CI, without a Git
whitespace gate. To enable the optional local pre-commit hook for a clone:

```sh
git config core.hooksPath .githooks
```

The hook runs `git diff --cached --check` on staged changes only. It does not
check unstaged changes or existing commits.

Generated contract types must be changed through the schema and
`cargo xtask generate`, not edited directly. Keep labels in the [translation
catalog](docs/LOCALIZATION.md); metadata and lyrics remain verbatim.

Demo tests require no account or network. Native checks are opt-in: `--live-check
--read-only` reads account data; `--live-check` also briefly plays music. Do not run
playback checks against another person's active session.

## Share useful reports

Include the macOS version, terminal, window size, theme, language and steps to
reproduce. Start with `--demo` when possible. Remove tokens, request headers,
personal library data and identifying paths from logs and screenshots.

Do not commit build output, credentials, private account data or downloaded media.
New tests should target observable behavior, including cancellation and terminal
restoration. Contributions are offered under the repository's MIT license.

`cargo xtask generate` updates contract DTOs and JSON Schema. `cargo xtask demo`
updates embedded synthetic assets; `cargo xtask previews` renders README images.
The task package is independent of the frontend and is not linked into muse.

Keep checks tied to observable failures: invalid protocol data, lost saved data,
stale playback actions and broken terminal cleanup. Do not add frozen layouts,
exhaustive presentation combinations or timing thresholds while the UI evolves.
