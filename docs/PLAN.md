# Current work

Deliver one macOS Apple Music executable with a Swift domain service, an independent
Ratatui UI and the approved terminal design. Only the primary agent implements.
Changes remain small logical commits; the normal text delta limit is 500 lines.

## Acceptance checklist

- [x] Backend-owned schema and generated Swift/Rust models enforce the dependency boundary.
- [x] Catalog search, details and Home use existing account authentication without developer keys.
- [x] Native playback, exact song transitions, queue controls and library reads work.
- [x] Saved playlists/favorites/history, LRCLIB lyrics and device controls are implemented.
- [x] Superseded saves retain confirmed state after a later failure; stale reads stay excluded.
- [x] Approved themes, panels, transparency, animations and keyboard/mouse behavior are retained.
- [x] Behavior, contract, terminal and publication checks exist.
- [x] Rust panics leave a bounded, terminal-safe cause after terminal restoration.
- [x] All documents describe current code, with an accurate feature supply map.
- [x] The final inspected source has a verified optimized executable and matching release report.

## Current unit and remaining work

- [x] C42 — Preserve independent bootstrap state and visible errors when app storage fails.

The primary agent implements and inspects directly. Private pre-public development
history stays local.

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
- [x] Footer/help settings hints are visible and clickable.
- [x] Optimized linked executable and final settings journey pass.

## Distribution acceptance

- [x] Missing-login errors show one short message and open Music directly.
- [x] Handoff happens once per failed-login episode; no password/token input in muse.
- [x] Returning users retry the original action; playlists and preferences are preserved.
- [x] Subscription and permission dashboards are excluded; existing OS access behavior stays intact.
- [x] A versioned Apple Silicon archive installs and runs through the public Homebrew tap.
- [x] Homebrew upgrade changes the executable without deleting user data.
- [x] Releases use immutable version tags and a verified, clean main commit.
- [x] Main uses short feature/fix branches, CI and linear merge history; no develop branch.
- [x] User documents describe supported usage without untested-platform disclaimers.

Each unit must pass the contract gate and affected behavior checks.
No native playback or account sign-out is used for automation. Final acceptance
includes actual brew installation, version/probe checks, CI, and release checksums.
