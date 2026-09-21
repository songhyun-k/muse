# Current state

- Current unit: C53 complete; Make Swift generation checks portable across formatter versions and run the full macOS 14 PR gate.

- Public repository: https://github.com/songhyun-k/muse. Use short branches and PRs for main.
- Product version is 0.3.1; publication is separate from source validation.
- v0.3.0 is the previous published release. Its bottle-less formula can trigger
  Homebrew build-tool checks even though the application is already compiled.
- The primary agent implements and inspects directly; no Workers or review agents.
- Preserve the user's running processes. Build publication replaces the executable
  atomically so an already-running process keeps its original inode.

## Implementation

One Swift executable links the Ratatui static library. Swift owns domain data,
MusicKit, providers and persistence; Rust owns UI state and terminal I/O. Generated
DTOs follow contract/api.json. No backend/frontend implementation imports cross the
boundary, and no daemon or separate runtime is required.

Caught Rust panics report an interface failure after terminal restoration, with
up to 512 payload characters sanitized by the existing terminal text filter and
an ellipsis when truncated. Non-text or unprintable payloads have explicit diagnostics.

Superseded control replies still merge confirmed store, player and volume state
through the existing sequence checks. They leave newer UI intentions, loading and
feedback alone; superseded reads and failures remain excluded.

The frontend cancels obsolete accepted reads through the existing backend command,
using reserved request slots. Reads and cancellation requests remain counted until
their own replies arrive; saved edits and playback commands are never cancelled by navigation.

Local playlists and favorites list newest additions first for recent order before
pagination. Name/artist sorting, playback history and playlist track order are preserved.

Catalog search, detail and Home use the bounded Apple web client with public/OS
account tokens held in memory. MusicKit provides playback and library access.
Successful authenticated web reads, including details, re-arm the login handoff;
local and cached reads do not reset it.
Native queue replacement selects a nonzero current entry only after assignment.
Playback resolves uncached catalog songs in batches of up to 300 IDs, preserving
reference order and duplicates; missing songs fail before changing the queue.
Artwork accepts HTTPS and native musicKit URLs through URLSession; some library
entries return empty data and show placeholders. Artwork and LRCLIB lyrics stream
through a shared bounded reader, cancelling responses above 8 MiB and 1 MiB
respectively without waiting for EOF. LRCLIB requests use a versionless `muse`
User-Agent. Both use app memory caches and shared URLSession HTTP caching, which
can also store cacheable HTTPS responses on disk; [PROVIDERS](PROVIDERS.md) describes
storage boundaries. Core Audio provides output-device volume and mute controls.

Service retains the library location and retries a failed open on the next store
access. Successful opens retain the same LibraryStore and exclusive writer lock;
corrupt files continue to return storage errors without being overwritten.

Locked or corrupt app storage leaves bootstrap as an explicit failure while session,
player and volume arrive through independent events. The failure remains a visible
notification after the main list loads; no empty successful store replaces missing data.

The scheduler preserves mutation order separately for storage, playback/queue and
volume. Network preparation for lyrics or saved collections does not hold playback
controls or volume. Play and authorization retain ordering across storage and playback.

The Korean/English UI has a persistent language choice (`I` / `--language`), five
token themes, independent panels, visible focus headers, terminal transparency,
Nerd/fallback icons, keyboard/mouse and reduced motion. `,` opens a live settings modal; footer and help provide clickable entry points. Music metadata, saved names
and lyrics remain untranslated. English lyric wrapping preserves words.
Text sanitization removes terminal controls before NFC normalization, and lyric
wrapping uses byte boundaries from the sanitized text.
Focused album and playlist details accept `P` for full playback and `S` for full
shuffle, including at 80×24. Keyboard and mouse use the same detail play command.

Demo data is original and fictional. Metadata lives in backend/Fixtures/demo.json;
`cargo xtask demo` generates artwork and embeds the complete corpus. Production
code uses embedded assets without reading neighboring repositories.

## Public preparation

The English README and Korean companion follow the codex-scope presentation with
actual renderer previews. MIT licensing, Yatoro attribution, contributor/security
notes, language docs and a macOS CI workflow are present. Dependency license texts
are bundled with the Apple Silicon preview archive. The macOS 15 CI runs focused behavior checks; the release workflow also verifies
optimized packaging on macOS 14. Homebrew creates a relocatable arm64_sonoma
bottle from that inspected executable, reinstalls it and verifies the receipt,
executable checksum, signature and demo probe before publication.

[VALIDATION](VALIDATION.md) describes retained behavior checks and account/distribution limits.

Current source and sensitive-pattern scans pass. Public main and source archives
contain only original demo media. Private development history remains local.
Source audit, export and publication verification preserve Git's NUL-delimited
path bytes, including leading whitespace; missing source files fail verification.
Packages use ad-hoc signing and retain the local.muse.cli bundle identifier.
Publication rejects existing version tags in the destination repository before
uploading artifacts; even an unpublished tag requires a new version.

## Commands and data

- Build: cargo xtask build --release
- Run: ./dist/muse --language en; preview: ./dist/muse --demo --language ko
- Check: cargo xtask check
- Generate contract: cargo xtask generate
- README previews: cargo xtask previews
- Package: cargo xtask release; cargo xtask bottle on a disposable macOS 14 arm64 GitHub runner
- Source audit/export: cargo xtask audit --history --export
- Private data: ~/Library/Application Support/muse/library.json; UI: ui.json.
  MUSE_STATE_DIR selects an absolute isolated directory; demo ignores these files.
- Native diagnostics: --live-check --read-only reads; --live-check briefly plays
  and stops. Diagnostic messages use English, preserve service error details and
  do not support --language. Do not interrupt the user's music as part of offline checks.
