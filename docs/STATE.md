# Current state

- Current unit: C15 complete; Remove frozen visual baselines and render-time gates.

- Public repository: https://github.com/songhyun-k/muse. Use short branches and PRs for main.
- v0.2.0 is released and Homebrew installation/upgrade are verified.
- The primary agent implements and inspects directly; no Workers or review agents.
- Preserve the user's running processes. Build publication replaces the executable
  atomically so an already-running process keeps its original inode.

## Implementation

One Swift executable links the Ratatui static library. Swift owns domain data,
MusicKit, providers and persistence; Rust owns UI state and terminal I/O. Generated
DTOs follow contract/api.json. No backend/frontend implementation imports cross the
boundary, and no daemon or separate runtime is required.

Catalog search, detail and Home use the bounded Apple web client with public/OS
account tokens held in memory. MusicKit provides playback and library access.
Native queue replacement selects a nonzero current entry only after assignment.
Artwork accepts HTTPS and native musicKit URLs through URLSession; some library
entries return empty data and show placeholders. Lyrics use LRCLIB; Core Audio
provides output-device volume and mute controls.

The Korean/English UI has a persistent language choice (`I` / `--language`), five
token themes, independent panels, visible focus headers, terminal transparency,
Nerd/fallback icons, keyboard/mouse and reduced motion. `,` opens a live settings modal; footer and help provide clickable entry points. Music metadata, saved names
and lyrics remain untranslated. English lyric wrapping preserves words.

Demo data is original and fictional. Metadata lives in backend/Fixtures/demo.json;
a small generator creates artwork and embeds the complete corpus. Production code uses embedded assets without reading neighboring repositories.

## Public preparation

The English README and Korean companion follow the codex-scope presentation with
actual renderer previews. MIT licensing, Yatoro attribution, contributor/security
notes, language docs and a macOS CI workflow are present. Dependency license texts
are bundled with the Apple Silicon preview archive. Hosted macOS 15 CI passes the
full offline suite and optimized packaging checks on Swift 6.1.2.

[VALIDATION](VALIDATION.md) describes retained behavior checks and account/distribution limits.

Current source and sensitive-pattern scans pass. Public main and source archives
contain only original demo media. Private development history remains local.
Packages use ad-hoc signing and retain the local.muse.cli bundle identifier.

## Commands and data

- Build: python3 scripts/build.py --release
- Run: ./dist/muse --language en; preview: ./dist/muse --demo --language ko
- Check: python3 scripts/check.py --full
- Package: python3 scripts/release.py
- Source audit/export: python3 scripts/public_check.py --history --export
- Private data: ~/Library/Application Support/muse/library.json; UI: ui.json.
  MUSE_STATE_DIR selects an absolute isolated directory; demo ignores these files.
- Native diagnostics: --live-check --read-only reads; --live-check briefly plays
  and stops. Do not interrupt the user's music as part of offline checks.
