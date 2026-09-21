# Current inspection

Inspection unit: C56

The primary agent inspected the changes since C50 and the terminal shutdown paths.

- Homebrew and formatter fixes retain the existing native build/publication tasks.
- Terminal shutdown reuses signal-hook and the libc already present in the dependency
  graph. Kernel events notify the host without consuming input or using timers.
  A first-wins exit stream keeps shutdown independent of UI completion; terminal
  restoration uses a private nonblocking descriptor rather than the stdout lock.
- Kernel writer locks and atomic domain writes remain unchanged. Playback history
  advances its deduplication marker only after persistence succeeds.
- Existing PTY and Swift behavior checks cover the failure and recovery paths.

No additional abstraction or dependency family is needed.
