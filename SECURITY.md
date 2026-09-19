# Security and privacy

muse uses the Apple Music account already signed in on the Mac. Web tokens are
kept in process memory. The app's `library.json` contains playlist items, favorites
and listening history; treat it as private. `ui.json` contains presentation
preferences. The app does not save audio files.

The Apple web integration is unofficial. Authenticated requests are restricted to
the configured Apple API host; image requests do not receive account tokens.
Artwork uses HTTPS or MusicKit's native image URL scheme. Remote metadata is
sanitized before it reaches the terminal. Demo mode does not access accounts,
network providers, device volume or the normal saved library.

If you find an authentication, data exposure or unsafe terminal-rendering issue,
use private vulnerability reporting on the public repository when available.
Do not put credentials, tokens or a copy of your library in a public issue. An
ordinary issue may describe a problem without sensitive details while arranging
a private report.

Build scripts sign locally with an ad-hoc identity by default. Developer ID
signing and notarization are separate distribution steps; local verification is
not a claim that Apple has notarized a build.
