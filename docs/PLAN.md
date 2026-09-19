# Current work

Deliver one macOS Apple Music executable with a Swift domain service, an independent
Ratatui UI and the approved terminal design. Only the primary agent implements.
Changes remain small logical commits; the normal text delta limit is 500 lines.

## Acceptance checklist

- [x] Backend-owned schema and generated Swift/Rust models enforce the dependency boundary.
- [x] Catalog search, details and Home use existing account authentication without developer keys.
- [x] Native playback, exact song transitions, queue controls and library reads work.
- [x] Saved playlists/favorites/history, LRCLIB lyrics and device controls are implemented.
- [x] Approved themes, panels, transparency, animations and keyboard/mouse behavior are retained.
- [x] Deterministic unit, contract, reference-cell, terminal and publication checks exist.
- [x] All documents describe current code, with an accurate feature supply map.
- [x] The final inspected source has a verified optimized executable and matching release report.

## Current unit and remaining work

- [x] C04 — Public repository, preview artifacts and hosted offline/release checks verified.
- [ ] C05 — Settings modal: reuse UI preferences, verify keyboard/mouse and build; no commit.

The primary agent performs implementation and inspection directly. Publish main
and the verified v0.1.0 preview; preserve private development history locally.
The settings modal is a separate local change and must remain uncommitted.

## Public preparation acceptance

- [x] English and Korean labels, menus, help, CLI and application errors are covered.
- [x] CLI language selection persists; old preferences remain readable.
- [x] Track metadata, playlist names and lyrics remain untranslated.
- [x] Narrow/wide, transparent and reduced-motion views work in both languages.
- [x] README follows the codex-scope presentation, with real renderer previews.
- [x] Licensing, media provenance, sensitive data, build and documentation are audited.
- [x] Relevant tests and the release build pass; unresolved publication issues are explicit.

At every commit, replace the completed-unit row with the newly completed unit;
keep remaining unit IDs contiguous. Update STATE with that same unit. Finished
work and superseded plans belong in Git history, not this checklist. Every fifth
commit replaces REVIEWS with the current simplification inspection. Record only
an applicable current size exception in commit-exceptions.json.
