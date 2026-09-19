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

- [x] C06 — Visible settings entry points, help and regression coverage.
- [ ] C07 — Login, Homebrew and branch/release acceptance plan.
- [ ] C08 — Typed account recovery states and permitted system actions.
- [ ] C09 — Login guidance, account recheck and recovery actions.
- [ ] C10 — First-run and returning-user journey checks.
- [ ] C11 — Versioned release packaging and repeatable publication.
- [ ] C12 — Homebrew tap, installation and upgrade validation.
- [ ] C13 — Release publication, repository rules and current documentation.

The primary agent implements and inspects directly. The user authorizes the
settings commits and implementation through Homebrew distribution. Private
pre-public development history stays local.

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

## Settings acceptance

- [x] `,` opens live language, theme, background, icons, motion and panel settings.
- [x] Existing UI preferences persist changes without backend-schema changes.
- [x] Keyboard, mouse, bilingual narrow/wide layouts and all themes are verified.
- [x] Footer/help settings hints are visible and clickable; all other reference cells stay exact.
- [x] Optimized linked executable and final settings journey pass.
