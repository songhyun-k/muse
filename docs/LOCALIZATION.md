# Localization

muse supports English (`en`) and Korean (`ko`). The interface selects Korean for
`ko` terminal locales and English otherwise, checking `LC_ALL`, `LC_MESSAGES`,
then `LANG`. A saved language takes precedence over the environment; an explicit
`--language en|ko` takes precedence over saved settings. Press `I` to change it
inside the app, or use the language row in Settings (`,`). Normal launches save the choice in `ui.json`; demo mode does not.

The Rust presentation layer owns the language preference. The Swift backend and
wire protocol do not depend on it. Existing preferences without a language field
remain readable and use locale detection on the next launch; saving the UI state
records that language.

## Text catalog

[frontend/assets/en.json](../frontend/assets/en.json) maps Korean source messages
to English. UI code opts in through `Ui::text`; known service errors and prefixed
OS errors pass through `Ui::message`. This is source-message localization, so an
application message change must update the catalog. No network translation or
localization dependency is used.

Song titles, artist names, album names, playlist names, search input and lyric
bodies are never translated. Language changes must not rewrite saved music data.
The embedded macOS music-permission description is bilingual.
CLI help uses an explicit `--language` or the terminal locale without loading saved preferences.
CLI help lives in `frontend/assets/help.en.txt` and `help.ko.txt`. Low-level host
startup diagnostics and protocol debugging output use English.

To add a language, add the locale/catalog and CLI help, then extend the language
picker, preference parsing and tests. Keep interpolated metadata outside catalog
lookups; plural units live in `Language::unit`. Check long labels at 80×24, sidebar
clipping, dialogs, help, errors and preserved terminal backgrounds.

## Checks

```sh
cargo test --locked --manifest-path frontend/Cargo.toml
```

The Rust checks cover locale selection, persisted settings, preserved music
metadata and localized service feedback. Presentation combinations and exact
labels are not development gates.
