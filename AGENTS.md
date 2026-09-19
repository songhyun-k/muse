# Working agreement

- Read `docs/PLAN.md`, `docs/ARCHITECTURE.md`, `docs/STATE.md` before resuming.
- The primary agent implements and inspects everything directly. Do not spawn
  implementation Workers, review agents or sub-agents unless the user asks.
- One logical change per commit, normally <=500 added + deleted text lines.
  Never compress formatting to fit the limit. Document essential exceptions.
- Update the checklist and STATE in every commit. Unit IDs stay in documents;
  commit subjects describe the change without a required prefix.
- PLAN contains the current completed unit and contiguous pending units; remove
  the previous completed row at each commit. Keep feature acceptance checkboxes.
- Documents describe current code and supported behavior. Keep work history and
  superseded decisions in Git, not repository documents. Update affected supply paths.
- Inspect simplification at every fifth commit; replace `docs/REVIEWS.md` with the
  current inspection and its `Inspection unit: Cnn` marker, without a running history.
  Refactor only where it improves actual readability or removes real duplication.
- `docs/commit-exceptions.json` contains only an applicable current size exception.
- Swift owns domain state, MusicKit, domain persistence and service I/O. Rust owns all
  presentation state and terminal I/O. Neither imports the other implementation.
- `contract/api.json` is the backend-owned, versioned public contract.
  Native DTOs are generated and checked for drift. No framework objects cross it.
- The host is the sole composition root. No networking server, daemon or plugin
  architecture unless a concrete requirement makes one necessary.
- Match the approved English/Korean UI's geometry, token colors and interactions.
  Freeze reference fixtures before changing the renderer. Do not redefine matches.
  References and demo media must use original synthetic data; no downloaded artwork
  or commercial lyrics. Baseline replacement requires explicit user authorization.
- No fabricated playback, audio analysis or library data in normal mode.
  Test/demo fixtures must be explicitly selected.
- Keep error handling, cancellation, accessibility and terminal cleanup intact.
- Run `scripts/check.py` and the affected build/tests before committing.
- Never commit keys, credentials, personal library data, build output or caches.
- Public source: https://github.com/songhyun-k/muse. Work on short feat/ or fix/
  branches and use CI + rebase-merged PRs for main. Publish only when the user asks.
  Never push private development branches or replace a published release.
- Follow docs/MAINTENANCE.md for version bumps, releases and Homebrew updates.
