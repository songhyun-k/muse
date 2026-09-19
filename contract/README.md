# Backend API v1

`api.json` is the sole source of truth, owned by the backend's public interface.
Both implementations depend only on their generated DTOs. Generated JSON Schema
uses draft 2020-12, `$defs`, strict objects and `type`/`data` tagged unions.

```json
{"version":1,"id":1,"command":{"type":"snapshot","data":{}}}
```

An accepted request gets exactly one correlated Event (`id` equals request ID).
Unsolicited events omit `id`. Sequence increases when an event is emitted, and is
scoped to one backend lifetime. IDs are nonzero and unique while outstanding;
they are transport correlation, never item identity. `cancel` settles its target
with `cancelled` immediately for a pending read and acknowledges itself; an already
completed target is a no-op. Mutations cannot be canceled after admission and return
`unavailable` to a cancel request. A canceled read never later publishes success,
even if its provider ignores cancellation; it still consumes an execution slot
until it actually finishes. UI reserves admission capacity for control/cancel.

Optional fields accept absence/null and are omitted by native encoders when nil.
Unknown fields/tags, missing required fields and incompatible versions fail.
Numbers must be finite; unsigned integers cannot be negative or fractional.
Strings contain at most 8192 Unicode scalars, arrays at most 32768 elements,
nesting at most 24 and the entire UTF-8 payload at most 1 MiB. Text is data:
the frontend strips terminal control sequences and uses Unicode cell widths.

Catalog search pages contain up to 25 items; library/detail/saved-list pages use
up to 50. Home is a curated selection of up to 50 recommendations.
Offset <=1,000,000; `nextOffset: null` means exhausted.
Search queries are trimmed, 1..200 scalars; collection names 1..100 scalars,
descriptions <=2000. A collection holds <=2000 tracks and the app <=100
collections. Favorites <=2000; history retains the latest 1000 plays.
Store summaries contain references; full collections/favorites/history use browse.

Play lists must be nonempty with an in-range startIndex. They are resolved by the
backend from ItemRef, never trusted frontend metadata. Queue entries have unique
IDs so duplicate songs can be independently removed/reordered. `beforeEntryId`
absent means move to the end. Storage mutations are atomic. Failed playback
replacement attempts restore the previous queue and publish the actual player
state; native playback/network side effects cannot be transactionally undone.
Seek seconds are finite and within the current track, volume is 0..1, lyric offset
is -30..30 seconds. Positive lyric offset delays the lyric by that many seconds.
Capabilities in session/player/volume govern which controls are enabled.

PlayerState contains only current playback and queue count/revision, not all queue
metadata. `updatedAt` is capture time in Unix seconds, allowing bounded frontend
position interpolation. `queue` returns up to 50 upcoming entries (excluding current
and past entries), with a revision and total. A pagination request carrying an old
revision returns `conflict`; the UI reloads instead of combining different queues.
QueueEntry.item may be absent while native metadata loads; its queue ID remains
valid, and metadata-dependent actions stay disabled. The revision changes when
upcoming order, current entry or metadata availability changes. This keeps
frequent playback events bounded even with 2000 songs. Notice tags also identify
independent unsolicited snapshot streams at the host mailbox.

Item identity is `(source, kind, id)`; IDs are opaque strings, not filesystem paths.
Library and catalog identities are not interchangeable. The backend stores item
metadata with saved references so unavailable songs remain visible after restart.
`artworkUrl` is display metadata; the frontend never fetches it. `artwork` obtains
a backend-decoded <=96x96 RGB image, returned as row-major bytes; missing art fails
explicitly. LRCLIB match IDs are supplied by its result, not arbitrary URLs.

UI state (theme, selection, panels, scroll, editing) is deliberately absent.
Normal mode cannot return fixture data as a fallback on a service failure.
