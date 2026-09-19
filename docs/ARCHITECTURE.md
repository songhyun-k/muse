# Implementation contract

## Responsibility and dependencies

The product is a macOS Apple Music client with the approved English/Korean terminal UI.
Original synthetic fixtures define renderer parity. Collection editing, search category/
pagination and authorization controls share that visual language and have additional
behavior tests without replacing the reference fixtures. [PROVIDERS](PROVIDERS.md)
maps each feature to its actual network, platform or storage provider.
Swift is a backend service, even though it runs inside the same process as the UI.
Ratatui owns navigation, selection, scroll, editing, focus, panel visibility,
themes, mouse interaction, animation timing and terminal capabilities.
Rust also persists UI-only preferences (language, theme, panels, motion); they never enter
the backend contract. `MUSE_STATE_DIR` optionally selects an absolute data directory
for both owners; they still open only their own files. This enables isolated normal
mode checks without replacing HOME or touching the user's files. Swift persists only domain data.

```text
Swift backend -> generated Swift contract <- contract/api.json
Rust frontend -> generated Rust contract  <- contract/api.json
Swift host    -> backend + small C transport ABI -> Rust static library
```

Only the host connects the implementations. The backend cannot import or call
the frontend. The frontend cannot import MusicKit or call backend implementation
symbols. Neither depends on the host. The contract contains data, never callbacks,
UI coordinates, Swift objects, Rust objects, or terminal state.

The host keeps Swift's main actor available for MusicKit. A dedicated terminal
thread runs Ratatui. Bounded, thread-safe message queues transport UTF-8 JSON;
commands, correlated replies and unsolicited state changes use the same schema.
No HTTP listener, separate daemon or extra software installation is required.
The Rust static library is linked into one Swift executable, using macOS system
frameworks. System frameworks are not claimed to be statically linked.

Admission and delivery never block the main actor. Accept at most 16 outstanding
requests and reserve a reply slot for each accepted request; completed unread
replies still count toward that limit. Return explicit busy/closed status before
acceptance. Replies are never silently dropped. Unsolicited snapshots coalesce by
event type and use a separate bounded mailbox; polling alternates replies/events.
Each message is at most 1 MiB. Closing rejects new work, cancels pending work and
wakes the UI; no callback may outlive its context. Saturation and exit are tested.
Dragging coalesces unsent seek/volume intent on the frontend.

## Public backend schema

`contract/api.json` defines protocol version, records, enums and tagged messages.
Generate native Codable/Serde DTOs and machine-readable JSON Schema from it.
Check generation deterministically; never hand-edit generated DTOs. Every request
has a protocol version and unique ID. Replies retain the ID. A duplicate of an
active ID fails without correlation so it cannot settle the accepted request.
Events have explicit types. Unknown commands, incompatible versions, invalid numbers, invalid IDs and
oversized messages return structured errors. Terminal escape/control characters
are never rendered from external metadata. Requests have cancellation/stale-result
handling. An error cannot be mistaken for an empty successful collection.

The backend is authoritative for playback, queue, collections, favorites, history,
lyrics results/offsets and device volume. The frontend only sends intentions and
renders confirmed results. Playback time may be interpolated between timestamps;
that interpolation never updates backend state. Selection/editing are frontend-only.
Seek carries the current native queue entry ID and fails if that entry changed.
Shuffled queue replacement is one play intention, not a second unconditional mode
command. Failed replacement restores the previous shuffle state with the queue.
Native replacement assigns the new queue before selecting a nonzero currentEntry;
entry identities preserve the exact start among duplicate songs.
Collection updates are patches: omitted fields are retained, an empty description
clears it, and at least one changed field is required.

## Product scope

- Search: songs, albums, artists, playlists, stations; browse and pagination.
- Library: recently added, songs, albums, artists, existing playlists (read-only).
- Home: account-recommended albums, playlists and stations from the web API.
- Details: metadata, artwork and playable tracks, back navigation.
- Playback: play/pause, previous/next, seek, shuffle/repeat, queue edit/reorder.
- Collections: create/rename/describe/copy/delete, add/remove/reorder songs.
- Favorites and application playback history persist across launches.
- Lyrics: LRCLIB plain/synchronized/instrumental/missing/error, matching and offset.
- Volume: Core Audio output volume/mute/name, capability-aware controls.
- UI: all approved demo views, keyboard/mouse, five themes, independent panels,
  transparent background, Nerd Font/fallback, resizing and reduced motion.

Apple Music export, cloud sync, downloads, DSP, real audio spectrum, Sing,
crossfade and quality selection remain outside this release. UI labels stay short
(플레이리스트, 즐겨찾기). The existing animated waveform is a decorative progress
visualization; it must never be described as audio measurement.

## Deterministic quality gates

1. Dependency scanner enforces module imports and manifest edges above. One
   implementation per concrete service; avoid speculative interfaces/factories.
2. Contract generator `--check`; round-trip shared fixtures in Swift and Rust;
   malformed/version/oversize cases rejected at the boundary.
3. `swift test`, `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings`.
   Tests target contract, data integrity and observable behavior, not code shape.
4. Frozen current UI snapshots at a fixed state/clock: 80x24 and 140x40;
   both languages, five themes, all navigation pages plus details/full lyrics,
   transparent and collapsed-panel variants.
   Compare glyphs, cell widths, foreground/background and emphasis. Artwork uses
   original synthetic data only in explicit demo mode. Animated frames use fixed
   timestamps: scalar tolerance 1e-6, every glyph/RGB/background/emphasis cell exact.
   No masks or visual-guesswork tolerance.
   Contract event fixtures drive Rust-only renderer tests. Explicit interactive
   demo mode uses a host fixture service; the production frontend
   consumes the same contract without implementing fake domain behavior.
5. PTY tests cover Korean input, mouse, resize, focus retention, exit, interruption,
   default background preservation and terminal restoration. No browser/CUA needed.
6. Persistence tests cover atomic replacement, malformed data and stable IDs;
   a failed write never reports success or destroys previous data.
7. Release inspection checks Mach-O dependencies, embedded Info.plist, contract
   version and clean launch without a separate Rust/Python runtime or config files.
8. At 140x40, render-only p95 <=16.7ms on this Mac; idle must block rather than spin.
   Animation scheduling stops when idle or reduced motion is selected.
   Timing and idle checks run in `scripts/check.py --full`.
9. Every commit replaces the current completed PLAN unit and updates STATE. A hook
   requires contiguous pending units and enforces <=500 changed text lines;
   unavoidable generated/lock/reference exceptions require a written reason.
10. Every fifth commit replaces the current simplification review, including an
    explicit skip when no useful refactor exists. Work history belongs in Git.

## Acceptance and runtime prerequisites

All checklist items and automated gates pass; the binary launches and the approved
UI matches frozen fixtures. Normal mode uses real services with honest failures.
Live checks distinguish authorization, web reads, native library and native playback.
The read-only mode constructs no player; full acceptance additionally requires exact
first-attempt song transitions, playback progress and observed control effects.
Fixture success never substitutes for required live acceptance.

The runtime uses macOS music permission and the existing signed-in account. Native
catalog playback checks subscription eligibility. Web reads obtain the public web
token and OS user token without manual developer credentials. This unofficial read
path is isolated in the backend and may require updates when Apple changes it.

The host embeds NSAppleMusicUsageDescription and a configurable bundle identifier.
Default ad-hoc signing works for local execution; distribution signing is configurable.
`--full` builds the linked binary and runs offline gates without initiating live playback.
Untrusted metadata is sanitized before terminal rendering.
