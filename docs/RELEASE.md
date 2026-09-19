# Build and use

The app targets macOS 14+. Apple Silicon is tested locally and in hosted macOS 15
CI with Xcode 16.4 / Swift 6.1.2. Intel is not yet verified. Building requires
Xcode with Swift 6, Rust 1.88+ and Python 3.
The resulting executable uses macOS frameworks and needs no extra language runtime.

```sh
python3 scripts/build.py --release
./dist/muse --demo --language en
./dist/muse --language ko
```

Normal playback uses the Mac's signed-in Apple Music account and music access
permission. Catalog playback requires a subscription. Search and Home use an
unofficial Apple web integration without user-supplied developer keys. Its behavior
can change independently of this app.

## Appearance and language

```sh
./dist/muse --theme graphite --transparent
./dist/muse --theme porcelain --reduced-motion
./dist/muse --plain-icons
./dist/muse --256-color
./dist/muse --help --language en
```

`--language en|ko` overrides saved language and terminal locale detection. `I` opens
the language picker. `,` or the footer settings hint opens Settings; select with ↑/↓ or Tab, change with ←/→,
Enter, Space or a mouse click, and close with Esc or ×. Changes apply immediately.
Normal launches save the selected language, theme, transparency,
icons, motion and panel state. Demo launches do not read or save these preferences.

Five themes are available: porcelain, graphite, linen, midnight and ink. Terminal
background mode keeps the terminal's own transparency/blur. It does not create
blur. Standard macOS Terminal selects 256-color output; true-color terminals can
use `--true-color`. Nerd Fonts provide the dedicated glyphs; `--plain-icons` uses
fallback symbols. Minimum size is 80×24; 140×40 leaves room for both panels.

`Tab` / `Shift-Tab` move focus, `[` / `]` toggle panels, `/` searches, `F` opens
view options, `Enter` opens or plays, and `Space` toggles playback. Use `N` to create
a playlist, `:` to edit, `a` to add a song, `f` to favorite, `M` to find lyrics,
`O` to adjust lyrics, `Ctrl-L` for full lyrics, `?` for help and `q` to quit.
Ctrl-C and INT/TERM/HUP restore the terminal before exit.

## Saved data

`~/Library/Application Support/muse/library.json` holds collections, favorites,
history and lyric matching/offset preferences. `ui.json` holds presentation settings.
An absolute `MUSE_STATE_DIR` selects an isolated directory. `--demo` ignores both
saved files and uses simulated playback/volume with original fictional music data.

The app's playlists and favorites do not sync to Apple Music. Apple Music playlist
export, cloud writes, downloads, audio-quality selection and Sing are not implemented.

## Verify and package

```sh
python3 scripts/check.py --full
python3 scripts/release.py
python3 scripts/public_check.py --history --export
```

The full check builds a debug binary, so packaging rebuilds the optimized executable
last. `release.py` verifies signing, linked system libraries, embedded metadata and
standalone launch from a different directory. It creates `dist/muse`, an architecture-
named binary archive (with license notices) and `dist/release.json`. The manifest reports the HEAD commit
and whether working-tree changes were present. Optimized builds remap compiler source paths and strip debug symbols before signing.
Archive owner IDs/names are normalized rather than exposing the build account.
No upload occurs.

`public_check.py --export` creates `dist/muse-source.tar.gz` from the current source,
including uncommitted files but excluding Git history, build output and ignored files.
The public main branch starts from the synthetic-media source tree. Local private
development branches are not published. The export tool neither rewrites history
nor creates a remote.

## Signing

The default bundle identifier is `local.muse.cli`; the default signature is ad-hoc.
This is suitable for local testing, not a claim of Developer ID signing or notarization.
[v0.1.0](https://github.com/songhyun-k/muse/releases/tag/v0.1.0) provides an Apple
Silicon preview archive, source archive and build manifest. It is not notarized;
macOS may require approval in System Settings → Privacy & Security. Homebrew is
not available.

To build with an installed signing identity and a bundle identifier you own:

```sh
MUSIC_BUNDLE_ID=com.yourcompany.muse \
MUSIC_SIGN_IDENTITY='Developer ID Application: Your Name (TEAMID)' \
python3 scripts/release.py
```

Notarization is a separate distribution step. See [Apple's signing guide](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac).

## Native service diagnostics

```sh
./dist/muse --live-check --read-only
./dist/muse --live-check
```

The first checks authorization, catalog search/detail, Home and library reads without
creating a player. The second also briefly plays songs, checks transitions, queue,
seek and playback modes, then stops. Both use an in-memory test store, read device
volume without changing it, and time out after 60 seconds. The full offline gate
never starts these account checks. Native diagnostics use technical output that is
separate from the localized interface. See [validation](VALIDATION.md) for current limits.
