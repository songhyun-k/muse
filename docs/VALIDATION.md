# Current validation

Local environment: Apple Silicon, macOS 26.6.2, Xcode 26.3 / Swift 6.2.4 and
Rust 1.98.1. The executable targets macOS 14+ on Apple Silicon.

[Hosted CI](https://github.com/songhyun-k/muse/actions/runs/35445774544) passes on
macOS 15 arm64, Xcode 16.4 / Swift 6.1.2, Rust 1.98.1 and Python 3.14.7. It runs the
full offline gate and optimized packaging checks; rendering stays within the 16.7ms budget.

## Automated checks

`python3 scripts/check.py --full` passes on the local Mac.

| Area | Result |
| :--- | :--- |
| Swift | 54 offline tests pass |
| Rust | 59 tests, rustfmt and Clippy with warnings denied pass |
| Contract / dependencies | Generated types, shared wire fixtures, input bounds and layer rules pass |
| Static UI | 240 Korean/English frames; all 939,200 cells match exactly |
| Motion | 180 frames; all 1,008,000 cells match; scalar tolerance 1e-6 |
| Rendering | 140×40 render p95 approximately 0.36ms; budget 16.7ms |
| Terminal | Keyboard, Korean input, mouse/drag/resize, transparency, idle behavior and signal restoration pass |
| Languages / settings | English/Korean help, errors, language picker, visible/clickable settings hints, live modal, saved preferences and metadata preservation pass |
| Storage / build publication | Atomic writes, corrupt-file preservation, open inode preservation and failed-probe rollback pass |
| Public source | Local links and source/history sensitive-pattern scans pass; no credential values are printed |

UI references contain original synthetic music data, generated cover art and
newly written lyrics. Checks never overwrite expectations. Timing measures rendering
only, excluding network and terminal I/O. PTY preferences checks use temporary
`MUSE_STATE_DIR` directories and do not change the user's real settings or library.

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

`python3 scripts/release.py` verifies the optimized executable's signature,
embedded metadata, absence of personal build paths, system-library dependencies
and relocated standalone launch.
Its binary archive includes the project license, attribution and Rust dependency
license texts. `dist/release.json` records architecture, checksum, signing and
whether the source tree contained uncommitted changes.

`python3 scripts/public_check.py --history --export` also creates a clean source
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
