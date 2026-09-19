# Contributing

muse is a macOS app with a Swift service and a Rust/Ratatui frontend. Keep changes
small and focused. Playback, providers and domain persistence belong to Swift;
view state, localization, preferences and terminal rendering belong to Rust.
The versioned interface is [contract/api.json](contract/api.json).

## Build and verify

Install Xcode with Swift 6, Rust 1.88+ and Python 3.11+. From a checkout:

```sh
python3 scripts/build.py
swift test --package-path backend
cargo test --locked --manifest-path frontend/Cargo.toml
cargo fmt --manifest-path frontend/Cargo.toml --check
cargo clippy --locked --manifest-path frontend/Cargo.toml --all-targets -- -D warnings
python3 scripts/check.py
./dist/muse --demo --language en
```

Generated contract types must be changed through the schema and
`scripts/generate.py`, not edited directly. Keep labels in the [translation
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
