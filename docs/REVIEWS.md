# Current inspection

Inspection unit: C30

The primary agent reviewed the tooling migration and retained behavior checks.

- A separate Rust task package owns generation, build, checks, previews and release
  tooling. It is never linked into the product and preserves frontend build paths.
- Existing Cargo, SwiftPM, codesign, plutil and otool perform their native jobs.
- Generated DTOs and JSON Schema are unchanged apart from generator comments.
  The decoded demo corpus, including artwork pixels, matches the previous data.
- Frozen cell references, animation replay, timing thresholds, presentation
  matrices and source-shape scanners are removed. No replacement visual gate exists.
- Native tests retain protocol bounds, data preservation, stale-action handling,
  accessibility behavior and terminal cleanup. The PTY check uses a real session
  without a screen emulator or timed UI journeys.
- Packaging checks exact archive contents, signatures, relocated execution and
  hashes. Publication requires the inspected clean remote-main commit; release
  creation and tap updates still require their explicit flags.
- The source audit continues to withhold sensitive values. Python commands were
  blocked during the final local behavior check; no tracked Python files remain.

No additional abstraction or broader refactor is needed.
