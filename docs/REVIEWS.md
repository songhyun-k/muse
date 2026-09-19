# Current inspection

Inspection unit: C35

The primary agent inspected the current tooling and dead-path cleanup.

- Source enumeration preserves Git's NUL-separated path bytes and fails on missing
  tracked files; the audit and archive share this one source list.
- Publication checks the exact remote version tag before writing or uploading.
  Existing tags are rejected without a second version registry or tag-peeling layer.
- Whitespace checking stays in the optional staged-change hook; CI no longer runs
  an empty working-tree diff check.
- Lyrics selection tests use the actual menu path instead of a test-only method.
  Unused translations and unnecessary full-history release checkout are removed.
- Existing native tools and dependencies remain sufficient. No visual snapshots,
  source-shape checks or generic workflow framework have been added.

No further abstraction or broad refactor is justified by these changes.
