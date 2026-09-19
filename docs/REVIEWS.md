# Current inspection

Inspection unit: C40

The primary agent inspected the combined frontend request and text paths.

- Confirmed state from superseded control replies uses the existing sequence
  merger without settling newer UI intentions or accepting stale read failures.
- Cancellation extends the same pending-request table. The combined completion
  path reads Pending.target and preserves confirmed control state; original and
  cancellation replies retain separate admission counts.
- Sanitization precedes NFC normalization, and wrapping obtains byte boundaries
  from the exact string it slices. Panic diagnostics reuse that filter after
  terminal restoration instead of adding a global hook or logging framework.
- Detail playback shortcuts call the existing action and preserve editor input.
  Lyrics-choice tests likewise follow the real menu action.
- Existing behavior checks cover these responsibilities without frozen layouts,
  new dependencies, duplicated state owners or a generic scheduling framework.
- Source paths and immutable release tags remain handled by Git and the existing
  tooling; optional whitespace checks stay local to the commit hook.

No additional abstraction or broad refactor is warranted.
