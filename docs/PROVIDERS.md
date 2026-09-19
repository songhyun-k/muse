# Feature supply map

Swift owns data providers, account access and domain persistence. Ratatui receives
contract messages and renders them; the UI performs no web or native music requests.

| Feature | Source | Authentication / storage | Implementation |
| :--- | :--- | :--- | :--- |
| Search | Apple web catalog: songs, albums, artists, playlists, stations | Public web token + OS account user token | [WebMusic](../backend/Sources/Backend/WebMusic.swift) |
| Home | Apple web recommendations | Same account tokens and storefront | [MusicDetails](../backend/Sources/Backend/MusicDetails.swift) |
| Catalog details | Web resources and tracks/albums relationships | Same web authentication | [CatalogDetails](../backend/Sources/Backend/CatalogDetails.swift) |
| Library | MusicLibraryRequest and native relationships | macOS music permission and existing account | [MusicLibrary](../backend/Sources/Backend/MusicLibrary.swift) |
| Playback, seek, modes | MusicKit ApplicationMusicPlayer; web batches for uncached catalog songs | Subscription eligibility; the system owns audio/DRM/decoding | [MusicPlayback](../backend/Sources/Backend/MusicPlayback.swift), [WebMusic](../backend/Sources/Backend/WebMusic.swift) |
| Queue | Native player queue and stable entry IDs | Duplicate tracks remain distinct entries | [MusicQueue](../backend/Sources/Backend/MusicQueue.swift) |
| Artwork address | Apple metadata from the corresponding catalog/library source | Web auth or native permission, depending on metadata source | [MusicService](../backend/Sources/Backend/MusicService.swift) |
| Artwork pixels | HTTPS CDN or native musicKit URL through URLSession | No account token is attached by the image loader; bounded app memory cache; HTTP caching described below | [Artwork](../backend/Sources/Backend/Artwork.swift) |
| Lyrics and matching | LRCLIB get/search/record endpoints | No API key; song, artist, album and duration are sent for matching | [LyricsService](../backend/Sources/Backend/LyricsService.swift) |
| Lyric timing | LRCLIB LRC timestamps plus confirmed player time | Chosen match and timing offset in app storage | [Service](../backend/Sources/Backend/Service.swift) |
| Volume and mute | Core Audio default output device | Local device capabilities and controls | [VolumeService](../backend/Sources/Backend/VolumeService.swift) |
| Playlists, favorites, history | App-owned LibraryStore; recent playlists/favorites ordered newest first before pagination | Atomic library.json; no cloud synchronization | [LibraryStore](../backend/Sources/Backend/LibraryStore.swift), [Collections](../backend/Sources/Backend/Collections.swift) |
| Language and appearance | Rust UI preference and embedded text/color catalogs | ui.json; Korean/English CLI and in-app selection | [i18n](../frontend/src/i18n.rs), [preferences](../frontend/src/preferences.rs) |
| Motion and waveform | Frontend time plus confirmed playback state | Decorative visualization, not measured audio | [scene](../frontend/src/scene.rs) |
| Demo | Embedded fictional fixtures and generated artwork | No account, provider network, device changes or normal saved files | [DemoService](../backend/Sources/Backend/DemoService.swift) |

## Credentials and ownership

MusicAuthorization checks access on the Mac. AppleWeb caches the public web token
in memory; MusicUserTokenProvider reuses the OS account authentication. There is
no app-managed web sign-in page or developer-key input. [Web backend](WEB_BACKEND.md)
describes request and refresh boundaries.

Both owners use `~/Library/Application Support/muse` by default, or the absolute
`MUSE_STATE_DIR`. Swift stores domain data in `library.json`; Rust stores UI
preferences in `ui.json`. The app does not persist account tokens or save audio files.

If app storage cannot open, [Service](../backend/Sources/Backend/Service.swift) publishes
session, player and volume events independently and returns the storage failure for
bootstrap. The UI keeps that failure visible after library/catalog rows load. Saved
data stays unavailable until a later store access successfully reopens storage;
the next access retries without restarting the application. A successful store keeps
its exclusive writer lock, and failed reads never replace corrupt data with an empty store.
Decoded artwork and loaded lyrics have bounded app-managed memory caches. Their
loaders use [`URLSession.shared`](https://developer.apple.com/documentation/foundation/urlsession/shared),
which uses the shared system `URLCache` and a
[default configuration](https://developer.apple.com/documentation/foundation/urlsessionconfiguration/default)
that permits disk caching. Cacheable HTTPS artwork and LRCLIB responses (lookup,
search and chosen matches) may also be stored on disk, subject to HTTP cache rules
and system policy. The app does not enforce memory-only response storage.
`MUSE_STATE_DIR` selects the app's JSON storage directory; it does not configure
the system HTTP cache.

Artwork and LRCLIB use the same [bounded response reader](../backend/Sources/Backend/ResponseBody.swift)
as AppleWeb. Artwork is limited to 8 MiB and lyrics to 1 MiB while streaming;
overflow cancels the download even without Content-Length or EOF.

Language selection changes app text only, never music metadata or lyrics.

Apple Music's own lyrics, cloud playlist creation/edit/export, and cloud syncing
are not connected. Existing library playlists can be read. Native artwork requests
can return an empty image for some library entries; the UI then shows a placeholder.

Missing OS account login is reported as `sign_in_required`. The Swift service opens
`com.apple.Music` once per failed-login episode and shows the existing short message.
Startup checks the OS user token without persisting it; network errors do not trigger
login handoff. A successfully decoded authenticated response in
[WebMusic](../backend/Sources/Backend/WebMusic.swift), including catalog details,
allows a later sign-out to trigger it again. Local and cached reads do not reset it.
