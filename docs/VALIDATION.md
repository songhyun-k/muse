# Current validation

Local environment: Apple Silicon, macOS 26.6.2, Xcode 26.3 / Swift 6.2.4 and
Rust 1.98.1. The executable targets macOS 14+ on Apple Silicon.

CI runs the native behavior suite on macOS 15 with the installed Xcode toolchain
and stable Rust. Optimized artifact inspection runs in the release workflow.

## Automated checks

`cargo xtask check` passes on the local Mac, including with Python commands
blocked in PATH. The tracked source contains no Python scripts.

| Area | Result |
| :--- | :--- |
| Swift | Offline behavior suite passes; live lyrics lookup remains opt-in |
| Rust | Frontend and tooling behavior suites, rustfmt and Clippy with warnings denied pass |
| Contract | Generated types, shared wire fixtures and input bounds pass |
| Terminal | Linked demo exits normally and after INT/TERM/HUP, restores terminal state and preserves saved files |
| Languages / settings | Locale selection, live settings, saved preferences and metadata preservation pass |
| Storage / build publication | Atomic writes, corrupt-file preservation, open inode preservation and failed-probe rollback pass |
| Public source | Source/history sensitive-pattern scans pass; a temporary Git repository checks leading-whitespace paths, archive inclusion, rename-independent secret detection and missing-file failure; no credential values are printed |

Publication's offline command-stub check rejects existing destination version tags
and tag-lookup failures before any release upload or tap write.

Demo media contain original synthetic music data, generated cover art and original
lyrics. Visual snapshots and timing thresholds are not development gates. PTY
checks use temporary `MUSE_STATE_DIR` directories and preserve real user data.
The panic diagnostic check covers string and non-text payloads, Unicode-safe
truncation, and removal of terminal controls from the cause reported after unwinding.

## Native services

The native artwork loader accepts HTTPS and MusicKit image URLs. Real library
samples decoded successfully through this loader; some native entries return an
empty image and still use a placeholder.

Native diagnostics are opt-in. `--live-check --read-only` checks catalog/detail,
Home and library reads without creating a player. `--live-check` additionally
plays briefly, checks song transitions, queue, seek and playback modes, then stops.
Both use an in-memory app store and never write the user's collections or history.


Apple's web integration remains unofficial. Packages use ad-hoc signing.
Hosted release validation covers the published Apple Silicon executable.

## Artifacts

`cargo xtask release` verifies the optimized executable's signature,
embedded metadata, absence of personal build paths, system-library dependencies
and relocated standalone launch.
Its binary archive includes the project license, attribution and Rust dependency
license texts. `dist/release.json` records architecture, checksum, signing and
whether the source tree contained uncommitted changes.

`cargo xtask audit --history --export` also creates a clean source
archive without `.git`. Public main and clean exports contain original synthetic
demo media. Private development branches remain local and are not distributed.

Missing-login checks use injected app-opening actions. They verify one handoff per
login episode, recovery after sign-in, no handoff for connectivity errors and
retrying the original search without adding a permission step.

## Installed release

[v0.2.0](https://github.com/songhyun-k/muse/releases/tag/v0.2.0) is an immutable stable
release. The [release workflow](https://github.com/songhyun-k/muse/actions/runs/35445774950)
passed the full gate, packaging checks, source comparison and publication checks.

The public tap passes `brew audit --strict` and `brew test`. Actual Homebrew upgrade
from 0.1.0 to 0.2.0 preserved isolated saved files. The installed executable passes
codesign verification and matches the published binary SHA-256. The tap also runs
[installation CI](https://github.com/songhyun-k/homebrew-tap/actions/runs/35446127083).
