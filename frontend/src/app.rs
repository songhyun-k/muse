use crate::{
    generated::*,
    requests::{Requests, Response, Target},
    state::{Data, Focus, Ui, View, item_key},
    transport::Admission,
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone)]
struct Intent {
    command: Command,
    target: Target,
}

struct Location {
    view: View,
    detail: Option<ItemRef>,
    library: bool,
}

#[derive(Default)]
pub struct App {
    pub ui: Ui,
    pub data: Data,
    pub requests: Requests,
    pub detail_ref: Option<ItemRef>,
    pub(crate) wanted_mode: Option<ModeParams>,
    pub(crate) wanted_volume: Option<VolumeParams>,
    pub(crate) wanted_position: Option<f64>,
    pub(crate) wanted_favorites: BTreeMap<String, bool>,
    pub(crate) search_due: Option<f64>,
    pub(crate) navigation_revision: u64,
    pub(crate) create_revision: Option<u64>,
    refresh_extent: usize,
    history: Vec<Location>,
    outbox: VecDeque<Intent>,
}

impl App {
    pub fn start(&mut self) {
        self.send(Command::Snapshot(Empty {}), Target::Bootstrap, 0);
        self.refresh(0);
    }

    pub fn send(&mut self, command: Command, target: Target, offset: u64) -> bool {
        self.outbox
            .retain(|intent| target == Target::Mutation || intent.target != target);
        if target != Target::Mutation {
            self.requests.invalidate(&target);
        }
        self.data.begin(target.clone(), offset);
        if self.outbox.len() >= 64 {
            self.local_failure(target, "처리 중입니다. 잠시 후 다시 시도해주세요");
            return false;
        }
        self.outbox.push_back(Intent { command, target });
        true
    }

    fn pending(&self, target: &Target) -> bool {
        !self.requests.pending_for(target).is_empty()
            || self.outbox.iter().any(|intent| &intent.target == target)
    }

    pub(crate) fn selection_ready(&self, target: &Target) -> bool {
        !self.data.loading.contains(target)
            && !self.data.errors.contains_key(target)
            && !self.pending(if *target == Target::Queue {
                &Target::QueueEdit
            } else {
                &Target::MainEdit
            })
    }

    /// Positional edits wait for mutations and the resulting visible-page refresh.
    pub(crate) fn list_ready(&self, target: &Target) -> bool {
        self.selection_ready(target)
            && [
                Target::Mutation,
                Target::Mode,
                Target::Create,
                Target::MainEdit,
                Target::QueueEdit,
            ]
            .iter()
            .all(|target| !self.pending(target))
    }

    fn local_failure(&mut self, target: Target, message: &str) {
        self.data.apply(Response {
            target: Some(target),
            sequence: 0,
            notice: Notice::Failure(Failure {
                code: ErrorCode::Busy,
                message: message.into(),
                retryable: true,
            }),
        });
    }

    /// A synchronous admission callback keeps the event loop testable without a native runtime.
    pub fn flush(
        &mut self,
        mut send: impl FnMut(&Request) -> Result<Admission, String>,
    ) -> Result<usize, String> {
        let mut sent = 0;
        while self.requests.count() < 16 {
            let (intent, index) = if let Some(request_id) = self.requests.next_cancel() {
                (
                    Intent {
                        command: Command::Cancel(CancelParams { request_id }),
                        target: Target::Cancel(request_id),
                    },
                    None,
                )
            } else {
                let next = self
                    .outbox
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.target.is_control() || self.requests.count() < 12)
                    .min_by_key(|(_, i)| match i.target {
                        ref target if target.is_control() => 0,
                        Target::Bootstrap | Target::Main => 1,
                        Target::Artwork(_) => 3,
                        _ => 2,
                    })
                    .map(|(i, _)| i);
                let Some(index) = next else {
                    break;
                };
                (self.outbox.remove(index).unwrap(), Some(index))
            };
            let request = self
                .requests
                .prepare(intent.command.clone(), intent.target.clone())?;
            match send(&request)? {
                Admission::Accepted => sent += 1,
                Admission::Busy => {
                    self.requests.rejected(request.id);
                    if let Some(index) = index {
                        self.outbox.insert(index, intent);
                    }
                    break;
                }
                Admission::Closed => return Err("음악 서비스 연결이 종료되었습니다".into()),
            }
        }
        Ok(sent)
    }

    pub fn receive(&mut self, event: Event) -> bool {
        let Some(response) = self.requests.complete(event) else {
            return false;
        };
        let previous = self.data.current().map(|i| i.r#ref.clone());
        if let (Some(Target::Favorite(key)), Notice::Store(store)) =
            (&response.target, &response.notice)
        {
            self.ui.feedback = Some(
                if store.favorites.iter().any(|item| item_key(item) == *key) {
                    "즐겨찾기에 추가"
                } else {
                    "즐겨찾기에서 제거"
                }
                .into(),
            );
        }
        let created = match (&response.target, &response.notice) {
            (Some(Target::Create), Notice::CollectionCreated(value))
                if self.create_revision == Some(self.navigation_revision) =>
            {
                Some(value.collection.clone())
            }
            _ => None,
        };
        if response.target == Some(Target::Create) {
            self.create_revision = None;
        }
        let authorized =
            matches!(&response.notice, Notice::Session(_)) && response.target.is_some();
        let previous_entry = self
            .data
            .player
            .as_ref()
            .and_then(|p| p.current_entry_id.clone());
        let main = response.target == Some(Target::Main);
        match &response.target {
            Some(Target::Mode) => self.wanted_mode = None,
            Some(Target::Volume) => self.wanted_volume = None,
            Some(Target::Seek) => self.wanted_position = None,
            Some(Target::Favorite(key)) => {
                self.wanted_favorites.remove(key);
            }
            _ => {}
        }
        let revision = self.data.player.as_ref().map(|p| p.queue_revision);
        let store_changed = matches!(
            response.notice,
            Notice::Store(_) | Notice::CollectionCreated(_)
        );
        self.data.apply(response);
        if main {
            if let Some(item) = &self.data.detail {
                self.detail_ref = Some(item.r#ref.clone());
            }
            if !self.data.errors.contains_key(&Target::Main)
                && self.data.items.len() < self.refresh_extent
                && let Some(offset) = self.data.next_offset
            {
                self.refresh(offset);
            } else {
                self.refresh_extent = 0;
                self.ui.cursors[0] =
                    self.ui.cursors[0].min(self.data.items.len().saturating_sub(1));
            }
        }
        if let Some(queue) = &self.data.queue {
            self.ui.cursors[1] = self.ui.cursors[1].min(queue.entries.len().saturating_sub(1));
        }
        if self.data.current().map(|i| &i.r#ref) != previous.as_ref() {
            self.invalidate_target(Target::Lyrics);
            self.invalidate_target(Target::LyricMatches);
            self.ui.lyric_manual = None;
            self.wanted_position = None;
            if matches!(self.ui.dialog, Some(crate::state::Dialog::Lyrics { .. })) {
                self.ui.dialog = None;
            }
        }
        if self.data.player.as_ref().map(|p| p.queue_revision) != revision {
            self.invalidate_target(Target::Queue);
        }
        if self
            .data
            .player
            .as_ref()
            .and_then(|p| p.current_entry_id.as_ref())
            != previous_entry.as_ref()
        {
            self.invalidate_target(Target::Seek);
            self.wanted_position = None;
        }
        if store_changed
            && matches!(
                self.ui.view,
                View::Playlists | View::Favorites | View::History
            )
        {
            self.reload_main();
        }
        if let Some(collection) = created {
            self.open(ItemRef {
                id: collection.id,
                source: Source::Collection,
                kind: Kind::Playlist,
            });
        }
        if authorized {
            self.refresh(0);
        }
        self.related();
        true
    }

    pub(crate) fn invalidate_target(&mut self, target: Target) {
        self.outbox.retain(|i| i.target != target);
        self.requests.invalidate(&target);
        self.data.cancel(&target);
    }

    pub fn go(&mut self, view: View) {
        if view == View::Lyrics && self.ui.view == view {
            return;
        }
        self.navigation_revision += 1;
        self.search_due = None;
        if view == View::Lyrics {
            self.save_location();
            if crate::state::NAV.contains(&self.ui.view) {
                self.ui.back = self.ui.view;
            }
            self.show_lyrics();
            return;
        }
        self.history.clear();
        self.ui.go(view);
        self.detail_ref = None;
        self.refresh(0);
    }

    fn show_lyrics(&mut self) {
        self.ui.go(View::Lyrics);
        self.detail_ref = None;
        self.refresh_extent = 0;
        self.data.begin(Target::Main, 0);
        self.invalidate_target(Target::Main);
    }

    pub(crate) fn leave_full_lyrics(&mut self) {
        if self.ui.view == View::Lyrics {
            self.back();
        }
    }

    pub fn open(&mut self, item: ItemRef) {
        self.ui.focus = Focus::Main;
        if self.detail_ref.as_ref() == Some(&item) {
            return;
        }
        self.navigation_revision += 1;
        self.search_due = None;
        self.save_location();
        if item.source == Source::Collection {
            self.ui.playlist_library = false;
        }
        if crate::state::NAV.contains(&self.ui.view) {
            self.ui.back = self.ui.view;
        }
        self.ui.go(if item.source == Source::Collection {
            View::Playlists
        } else {
            View::Detail
        });
        self.detail_ref = Some(item);
        self.refresh(0);
    }

    fn save_location(&mut self) {
        // ponytail: keep 32 locations and reload pages; cache page windows only if back latency matters.
        if self.history.len() == 32 {
            self.history.remove(0);
        }
        self.history.push(Location {
            view: self.ui.view,
            detail: self.detail_ref.clone(),
            library: self.ui.playlist_library,
        });
    }

    pub fn back(&mut self) {
        self.navigation_revision += 1;
        self.search_due = None;
        if let Some(location) = self.history.pop() {
            self.ui.playlist_library = location.library;
            if location.view == View::Lyrics {
                self.show_lyrics();
                return;
            }
            self.ui.go(location.view);
            self.detail_ref = location.detail;
            self.refresh(0);
        } else if self.detail_ref.is_some() || self.ui.view == View::Lyrics {
            self.go(self.ui.back);
        }
    }

    pub fn dispatch_due(&mut self, now: f64) {
        if self.search_due.is_some_and(|deadline| now >= deadline) {
            self.search_due = None;
            self.refresh(0);
        }
    }

    fn reload_main(&mut self) {
        let extent = self.refresh_extent.max(self.data.items.len());
        self.refresh(0);
        self.refresh_extent = extent;
    }

    pub fn refresh(&mut self, offset: u64) {
        if offset == 0 {
            self.refresh_extent = 0;
        }
        if self.ui.view == View::Lyrics {
            self.related();
            return;
        }
        let command = if let Some(item) = &self.detail_ref {
            Command::Detail(DetailParams {
                item: item.clone(),
                offset,
            })
        } else if self.ui.view == View::Search {
            if self.ui.query.trim().is_empty() {
                self.outbox.retain(|i| i.target != Target::Main);
                self.requests.invalidate(&Target::Main);
                self.data.begin(Target::Main, 0);
                self.data.apply(Response {
                    target: Some(Target::Main),
                    sequence: 0,
                    notice: Notice::Page(Page {
                        items: vec![],
                        next_offset: None,
                    }),
                });
                return;
            }
            Command::Search(SearchParams {
                query: self.ui.query.trim().into(),
                source: self.ui.search_source,
                kind: self.ui.search_kind,
                offset,
            })
        } else {
            let (scope, kind) = match self.ui.view {
                View::Home => (Scope::Home, Kind::Album),
                View::Recent => (Scope::Recent, Kind::Album),
                View::Artists => (Scope::Library, Kind::Artist),
                View::Albums => (Scope::Library, Kind::Album),
                View::Playlists => (
                    if self.ui.playlist_library {
                        Scope::Library
                    } else {
                        Scope::Collections
                    },
                    Kind::Playlist,
                ),
                View::Favorites => (Scope::Favorites, Kind::Song),
                View::History => (Scope::History, Kind::Song),
                _ => (Scope::Library, Kind::Song),
            };
            Command::Browse(BrowseParams {
                scope,
                kind,
                collection_id: None,
                order: self.ui.order,
                offset,
            })
        };
        self.send(command, Target::Main, offset);
    }

    pub fn more(&mut self) {
        if !self.data.loading.contains(&Target::Main)
            && let Some(offset) = self.data.next_offset
        {
            self.refresh(offset);
        }
    }

    pub fn related(&mut self) {
        if let Some(item) = self.data.current().cloned()
            && self.data.lyrics.is_none()
            && !self.data.loading.contains(&Target::Lyrics)
            && !self.data.errors.contains_key(&Target::Lyrics)
        {
            self.send(
                Command::Lyrics(LyricsParams { item: item.r#ref }),
                Target::Lyrics,
                0,
            );
        }
        if self.ui.right_open
            && self.ui.panel == crate::state::Panel::Queue
            && self.data.queue.is_none()
            && !self.data.loading.contains(&Target::Queue)
            && !self.data.errors.contains_key(&Target::Queue)
        {
            self.queue_page(0);
        }
    }

    pub fn queue_page(&mut self, offset: u64) {
        let revision = self.data.player.as_ref().map(|p| p.queue_revision);
        self.send(
            Command::Queue(QueueParams { offset, revision }),
            Target::Queue,
            offset,
        );
    }

    pub fn prefetch_artwork(&mut self, mut items: Vec<Item>) {
        if let Some(current) = self.data.current() {
            items.sort_by_key(|item| item.r#ref != current.r#ref);
        }
        // ponytail: 64 visible covers; larger views keep placeholders beyond this cap.
        // Raise the shared cache budget only if very large terminal views need more.
        let mut needed = BTreeMap::new();
        for item in items {
            if item.artwork_url.is_some() {
                needed.entry(item_key(&item.r#ref)).or_insert(item);
                if needed.len() == 64 {
                    break;
                }
            }
        }
        let obsolete: Vec<_> = self
            .data
            .loading
            .iter()
            .chain(self.data.errors.keys())
            .filter(|target| matches!(target, Target::Artwork(key) if !needed.contains_key(key)))
            .cloned()
            .collect();
        for target in obsolete {
            self.invalidate_target(target);
        }
        self.data.artwork.retain(|key, _| needed.contains_key(key));
        for item in needed.values() {
            self.image(item);
        }
    }

    fn image(&mut self, item: &Item) {
        if item.artwork_url.is_none() {
            return;
        }
        let key = item_key(&item.r#ref);
        let target = Target::Artwork(key.clone());
        if !self.data.artwork.contains_key(&key)
            && !self.data.loading.contains(&target)
            && !self.data.errors.contains_key(&target)
        {
            self.send(
                Command::Artwork(ArtworkParams {
                    item: item.r#ref.clone(),
                }),
                target,
                0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_login_retries_the_same_search_without_an_authorization_step() {
        use crate::{
            art::ArtCache,
            geometry::Visual,
            scene::{Action, Scene},
            theme::Palette,
        };
        let mut app = App::default();
        app.ui.view = View::Search;
        app.ui.query = "Mira".into();
        app.data.apply(Response {
            target: Some(Target::Main),
            sequence: 1,
            notice: Notice::Failure(Failure {
                code: ErrorCode::SignInRequired,
                message: "음악 앱에 로그인해주세요".into(),
                retryable: true,
            }),
        });
        let visual = Visual::settled(&app.ui, &app.data, 0.0);
        let palette = Palette::new(0, false);
        let mut art = ArtCache::default();
        let mut scene = Scene::new(&app.ui, &app.data, &visual, &palette, &mut art, 80, 24);
        scene.draw();
        assert!(
            scene
                .hits
                .iter()
                .any(|hit| matches!(hit.action, Action::Key("retry")))
        );
        assert!(
            !scene
                .hits
                .iter()
                .any(|hit| matches!(hit.action, Action::Key("authorize")))
        );
        let layout = scene.layout;
        app.key("enter", layout, 1.0, 1.0);
        let mut retried = false;
        app.flush(|request| {
            assert!(!matches!(request.command, Command::Authorize(_)));
            if let Command::Search(params) = &request.command {
                assert_eq!(params.query, "Mira");
                retried = true;
            }
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert!(retried && app.ui.dialog.is_none());
    }
    #[test]
    fn granted_authorization_reloads_library_even_when_subscription_is_unknown() {
        let mut app = App::default();
        app.data.errors.insert(
            Target::Main,
            Failure {
                code: ErrorCode::NotAuthorized,
                message: "접근 허용".into(),
                retryable: false,
            },
        );
        app.send(Command::Authorize(Empty {}), Target::Mutation, 0);
        let mut id = 0;
        app.flush(|r| {
            id = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        app.receive(Event {
            version: 1,
            id: None,
            sequence: 1,
            event: Notice::Failure(Failure {
                code: ErrorCode::Network,
                message: "구독 조회 실패".into(),
                retryable: true,
            }),
        });
        app.receive(Event {
            version: 1,
            id: Some(id),
            sequence: 2,
            event: Notice::Session(SessionState {
                authorization: Authorization::Authorized,
                can_play_catalog: None,
            }),
        });
        assert_eq!(
            app.data.session.as_ref().unwrap().authorization,
            Authorization::Authorized
        );
        assert!(!app.data.errors.contains_key(&Target::Main));
        assert!(app.data.loading.contains(&Target::Main));
        assert_eq!(
            app.data.notification_error().unwrap().1.message,
            "구독 조회 실패"
        );
        assert_eq!(
            app.flush(|r| {
                assert!(matches!(&r.command, Command::Browse(p) if p.scope == Scope::Library));
                Ok(Admission::Accepted)
            })
            .unwrap(),
            1
        );
    }

    #[test]
    fn artwork_demand_excludes_compact_rows_and_settles_above_cache_capacity() {
        use crate::{art::ArtCache, geometry::Visual, scene::Scene, state::Panel, theme::Palette};
        let mut app = App::default();
        app.data.items = (0..100)
            .map(|id| {
                serde_json::from_value::<Item>(serde_json::json!({
            "ref":{"id":format!("row-{id}"),"kind":"song","source":"catalog"},
            "title":id.to_string(),"artist":"a","album":"a","artworkUrl":"https://example.test/art"
        })).unwrap()
            })
            .collect();
        let rendered = |app: &App, height| {
            let visual = Visual::settled(&app.ui, &app.data, 0.0);
            let palette = Palette::new(0, false);
            let mut cache = ArtCache::default();
            let mut scene = Scene::new(
                &app.ui, &app.data, &visual, &palette, &mut cache, 140, height,
            );
            scene.draw();
            scene.artwork
        };
        app.ui.panel = Panel::Queue;
        app.data.queue = Some(QueuePage {
            entries: app
                .data
                .items
                .iter()
                .map(|i| QueueEntry {
                    id: i.r#ref.id.clone(),
                    item: Some(i.clone()),
                })
                .collect(),
            next_offset: None,
            revision: 1,
            total: 100,
        });
        assert!(rendered(&app, 35).is_empty());
        app.ui.view = View::Detail;
        let mut hero = app.data.items[0].clone();
        hero.r#ref.id = "hero".into();
        hero.r#ref.kind = Kind::Album;
        app.data.detail = Some(hero);
        let demand = rendered(&app, 80);
        assert_eq!(demand.len(), 1);
        assert_eq!(demand[0].r#ref.id, "hero");
        app.ui.view = View::Songs;
        app.data.detail = None;
        app.ui.right_open = false;
        let demand = rendered(&app, 180);
        assert!(demand.len() > 64);
        // Higher-sorted old keys used to evict still-visible lower-sorted keys forever.
        for id in 0..64 {
            let mut reference = app.data.items[0].r#ref.clone();
            reference.id = format!("zz-old-{id}");
            app.data.artwork.insert(
                item_key(&reference),
                Artwork {
                    item: reference,
                    width: 1,
                    height: 1,
                    rgb: vec![0, 0, 0],
                },
            );
        }
        let mut delivered = 0;
        let mut sequence = 0;
        for _ in 0..10 {
            app.prefetch_artwork(demand.clone());
            let mut requests = vec![];
            app.flush(|r| {
                requests.push(r.clone());
                Ok(Admission::Accepted)
            })
            .unwrap();
            for request in requests {
                let Command::Artwork(params) = request.command else {
                    panic!("unexpected read")
                };
                delivered += 1;
                sequence += 1;
                app.receive(Event {
                    version: 1,
                    id: Some(request.id),
                    sequence,
                    event: Notice::Artwork(Artwork {
                        item: params.item,
                        width: 1,
                        height: 1,
                        rgb: vec![1, 2, 3],
                    }),
                });
            }
        }
        assert_eq!(delivered, 64);
        assert_eq!(app.data.artwork.len(), 64);
        assert!(app.requests.is_idle() && app.data.loading.is_empty());
        app.prefetch_artwork(demand);
        assert_eq!(app.flush(|_| panic!("visible cache thrashed")).unwrap(), 0);
    }

    #[test]
    fn confirmed_collection_edits_restore_loaded_pages_before_clamping_selection() {
        let mut app = App::default();
        let song = |id: usize| {
            serde_json::from_value::<Item>(serde_json::json!({
                "ref":{"id":id.to_string(),"kind":"song","source":"catalog"},
                "title":id.to_string(),"artist":"a","album":"a"
            }))
            .unwrap()
        };
        app.ui.view = View::Playlists;
        app.detail_ref = Some(ItemRef {
            id: "p".into(),
            kind: Kind::Playlist,
            source: Source::Collection,
        });
        app.data.items = (0..150).map(song).collect();
        app.ui.cursors[0] = 120;
        let mut stored = app.data.items.clone();
        let mut sequence = 0;
        for action in ["remove", "move", "favorite"] {
            match action {
                "remove" => app.remove_collection_song(),
                "move" => app.move_collection_song(true),
                _ => app.favorite_selected(1.0),
            }
            let mut edit = None;
            app.flush(|r| {
                edit = Some(r.clone());
                Ok(Admission::Accepted)
            })
            .unwrap();
            let edit = edit.unwrap();
            match edit.command {
                Command::CollectionRemove(p) => {
                    stored.remove(p.index as usize);
                }
                Command::CollectionMove(p) => {
                    let item = stored.remove(p.from as usize);
                    stored.insert(p.to as usize, item);
                }
                Command::Favorite(_) => {}
                _ => panic!("unexpected edit"),
            }
            let selected = app.ui.cursors[0];
            sequence += 1;
            app.receive(Event {
                version: 1,
                id: Some(edit.id),
                sequence,
                event: Notice::Store(StoreState {
                    collections: vec![],
                    favorites: vec![],
                    history_count: 0,
                }),
            });
            let mut offsets = vec![];
            while app.data.loading.contains(&Target::Main) {
                assert_eq!(app.ui.cursors[0], selected);
                app.remove_collection_song();
                let mut page = None;
                app.flush(|r| {
                    page = Some(r.clone());
                    Ok(Admission::Accepted)
                })
                .unwrap();
                let page = page.unwrap();
                let Command::Detail(params) = page.command else {
                    panic!("edit against incomplete page")
                };
                offsets.push(params.offset);
                let start = params.offset as usize;
                let end = (start + 50).min(stored.len());
                sequence += 1;
                app.receive(Event {
                    version: 1,
                    id: Some(page.id),
                    sequence,
                    event: Notice::Page(Page {
                        items: stored[start..end].to_vec(),
                        next_offset: (end < stored.len()).then_some(end as u64),
                    }),
                });
            }
            assert_eq!(offsets, [0, 50, 100]);
            assert_eq!(app.data.items, stored);
            assert_eq!(app.ui.cursors[0], selected);
            assert_eq!(app.selected().unwrap().title, "121");
        }
        app.go(View::Songs);
        assert_eq!(app.refresh_extent, 0);
        assert_eq!(app.ui.cursors[0], 0);
    }

    #[test]
    fn structural_edits_wait_for_confirmation_and_the_matching_page() {
        let mut app = App::default();
        let item = |id: &str| {
            serde_json::from_value::<Item>(serde_json::json!({
                "ref":{"id":id,"kind":"song","source":"catalog"},
                "title":id,"artist":"a","album":"a"
            }))
            .unwrap()
        };
        app.ui.view = View::Playlists;
        app.detail_ref = Some(ItemRef {
            id: "p".into(),
            kind: Kind::Playlist,
            source: Source::Collection,
        });
        app.data.items = vec![item("a"), item("b"), item("c")];
        app.remove_collection_song();
        app.ui.cursors[0] = 1;
        app.remove_collection_song();
        let mut commands = vec![];
        app.flush(|r| {
            commands.push(r.clone());
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert_eq!(commands.len(), 1);
        let Command::CollectionRemove(params) = &commands[0].command else {
            panic!()
        };
        let mut stored = app.data.items.clone();
        stored.remove(params.index as usize);
        assert_eq!(
            stored.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            ["b", "c"]
        );
        app.remove_collection_song();
        assert_eq!(app.flush(|_| panic!("edit before reply")).unwrap(), 0);
        app.receive(Event {
            version: 1,
            id: Some(commands[0].id),
            sequence: 1,
            event: Notice::Store(StoreState {
                collections: vec![],
                favorites: vec![],
                history_count: 0,
            }),
        });
        app.remove_collection_song();
        let mut page = 0;
        app.flush(|r| {
            assert!(matches!(r.command, Command::Detail(_)));
            page = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        app.receive(Event {
            version: 1,
            id: Some(page),
            sequence: 2,
            event: Notice::Page(Page {
                items: stored,
                next_offset: None,
            }),
        });
        assert!(app.list_ready(&Target::Main));
        app.data.queue = Some(QueuePage {
            entries: ["a", "b", "c", "d"]
                .iter()
                .map(|id| QueueEntry {
                    id: (*id).into(),
                    item: Some(item(id)),
                })
                .collect(),
            next_offset: None,
            revision: 1,
            total: 4,
        });
        app.move_queue(true);
        app.move_queue(true);
        commands.clear();
        app.flush(|r| {
            commands.push(r.clone());
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert_eq!(commands.len(), 1);
        assert!(
            matches!(&commands[0].command, Command::QueueMove(p) if p.entry_id == "a" && p.before_entry_id.as_deref() == Some("c"))
        );
        app.ui.focus = crate::state::Focus::Right;
        app.ui.panel = crate::state::Panel::Queue;
        assert!(app.selected().is_none());
        app.data.begin(Target::Queue, 0);
        let request = app
            .requests
            .prepare(
                Command::Queue(QueueParams {
                    offset: 0,
                    revision: Some(1),
                }),
                Target::Queue,
            )
            .unwrap();
        app.receive(Event {
            version: 1,
            id: Some(commands[0].id),
            sequence: 3,
            event: Notice::Ack(Ack {
                message: String::new(),
            }),
        });
        app.move_queue(true);
        assert_eq!(
            app.flush(|_| panic!("edit before refreshed page")).unwrap(),
            0
        );
        let mut queue = app.data.queue.clone().unwrap();
        queue.entries.swap(0, 1);
        app.receive(Event {
            version: 1,
            id: Some(request.id),
            sequence: 4,
            event: Notice::Queue(queue),
        });
        assert_eq!(app.selected().unwrap().title, "a");
        app.move_queue(true);
        assert_eq!(app.flush(|r| {
            assert!(matches!(&r.command, Command::QueueMove(p) if p.entry_id == "a" && p.before_entry_id.as_deref() == Some("d")));
            Ok(Admission::Accepted)
        }).unwrap(), 1);
    }

    #[test]
    fn busy_transport_preserves_intent_and_navigation_replaces_unsent_reads() {
        let mut app = App::default();
        app.go(View::Songs);
        app.go(View::Albums);
        assert_eq!(app.outbox.len(), 1);
        assert_eq!(app.flush(|_| Ok(Admission::Busy)).unwrap(), 0);
        assert!(app.requests.is_idle());
        let mut sent = vec![];
        app.flush(|r| {
            sent.push(r.clone());
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert!(matches!(&sent[0].command, Command::Browse(p) if p.kind == Kind::Album));
        app.go(View::Favorites);
        assert!(!app.receive(Event {
            version: 1,
            id: Some(sent[0].id),
            sequence: 1,
            event: Notice::Page(Page {
                items: vec![],
                next_offset: None
            })
        }));
        assert!(app.data.loading.contains(&Target::Main));
    }

    #[test]
    fn obsolete_artwork_cancels_before_favorites_and_stays_counted_until_replies() {
        let mut app = App::default();
        let items = (0..12)
            .map(|id| {
                serde_json::from_value::<Item>(serde_json::json!({
                    "ref":{"id":id.to_string(),"kind":"song","source":"catalog"},
                    "title":"cover","artist":"a","album":"a",
                    "artworkUrl":"https://example.test/art"
                }))
                .unwrap()
            })
            .collect();
        app.prefetch_artwork(items);
        let mut reads = vec![];
        app.flush(|r| {
            reads.push(r.id);
            assert!(matches!(r.command, Command::Artwork(_)));
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert_eq!(reads.len(), 12);
        app.prefetch_artwork(vec![]);
        app.go(View::Favorites);
        let mut attempted = None;
        assert_eq!(
            app.flush(|r| {
                let Command::Cancel(p) = &r.command else {
                    panic!("expected cancellation")
                };
                attempted = Some(p.request_id);
                Ok(Admission::Busy)
            })
            .unwrap(),
            0
        );
        assert_eq!(attempted, Some(reads[0]));
        assert_eq!(app.requests.count(), 12);

        let mut cancelled = vec![];
        let mut browsed = false;
        let mut sequence = 0;
        while cancelled.len() < reads.len() {
            let before = app.requests.count();
            let mut sent = vec![];
            app.flush(|r| {
                sent.push(r.clone());
                Ok(Admission::Accepted)
            })
            .unwrap();
            assert!(!sent.is_empty());
            assert_eq!(app.requests.count(), before + sent.len());
            assert!(app.requests.count() <= 16);
            for request in sent {
                let Command::Cancel(p) = request.command else {
                    panic!("expected cancellation")
                };
                assert!(!cancelled.contains(&p.request_id));
                cancelled.push(p.request_id);
                // Even an early cancel acknowledgement cannot settle the original request.
                sequence += 1;
                assert!(!app.receive(Event {
                    version: API_VERSION,
                    id: Some(request.id),
                    sequence,
                    event: Notice::Ack(Ack {
                        message: "cancelled".into()
                    }),
                }));
            }
            assert_eq!(app.requests.count(), 12);
        }
        assert_eq!(cancelled, reads);
        assert_eq!(
            app.flush(|_| panic!("duplicate cancellation or early read"))
                .unwrap(),
            0
        );
        for id in reads {
            sequence += 1;
            assert!(!app.receive(Event {
                version: API_VERSION,
                id: Some(id),
                sequence,
                event: Notice::Failure(Failure {
                    code: ErrorCode::Cancelled,
                    message: "cancelled".into(),
                    retryable: true,
                }),
            }));
        }
        assert!(app.requests.is_idle());
        app.flush(|r| {
            assert!(matches!(&r.command, Command::Browse(p) if p.scope == Scope::Favorites));
            browsed = true;
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert!(browsed && app.data.errors.is_empty());
    }

    #[test]
    fn reads_leave_capacity_for_control_and_drags_coalesce_unsent_intent() {
        let mut app = App::default();
        for i in 0..20 {
            app.send(
                Command::Snapshot(Empty {}),
                Target::Artwork(i.to_string()),
                0,
            );
        }
        assert_eq!(app.flush(|_| Ok(Admission::Accepted)).unwrap(), 12);
        for seconds in [1.0, 2.0, 3.0] {
            app.send(
                Command::Seek(SeekParams {
                    seconds,
                    entry_id: "entry".into(),
                }),
                Target::Seek,
                0,
            );
        }
        assert_eq!(
            app.flush(|r| {
                assert!(matches!(&r.command, Command::Seek(p) if p.seconds == 3.0));
                Ok(Admission::Accepted)
            })
            .unwrap(),
            1
        );
    }

    #[test]
    fn changing_tracks_replaces_inflight_lyrics_without_waiting_for_the_old_track() {
        let mut app = App::default();
        let mut player: PlayerState = serde_json::from_value(serde_json::json!({
            "current":{"ref":{"id":"a","kind":"song","source":"library"},"title":"a","artist":"a","album":"a"},
            "playing":true,"position":0,"queueCount":0,"queueRevision":1,"updatedAt":0,
            "repeatMode":"off","shuffle":false,"canSeek":false
        })).unwrap();
        app.receive(Event {
            version: 1,
            id: None,
            sequence: 1,
            event: Notice::Player(player.clone()),
        });
        let old_item = player.current.as_ref().unwrap().r#ref.clone();
        let mut old_id = 0;
        app.flush(|r| {
            old_id = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        player.current.as_mut().unwrap().r#ref.id = "b".into();
        app.receive(Event {
            version: 1,
            id: None,
            sequence: 2,
            event: Notice::Player(player),
        });
        assert!(app.data.loading.contains(&Target::Lyrics));
        let mut cancellation = None;
        assert_eq!(
            app.flush(|r| {
                match &r.command {
                    Command::Cancel(p) => {
                        assert_eq!(p.request_id, old_id);
                        cancellation = Some(r.id);
                    }
                    Command::Lyrics(p) => assert_eq!(p.item.id, "b"),
                    _ => panic!("unexpected request"),
                }
                Ok(Admission::Accepted)
            })
            .unwrap(),
            2
        );
        assert!(!app.receive(Event {
            version: 1,
            id: Some(old_id),
            sequence: 3,
            event: Notice::Lyrics(Lyrics {
                item: old_item,
                match_id: None,
                status: LyricsStatus::Missing,
                lines: vec![],
                offset: 0.0
            })
        }));
        assert!(!app.receive(Event {
            version: API_VERSION,
            id: Some(cancellation.unwrap()),
            sequence: 4,
            event: Notice::Ack(Ack {
                message: "cancelled".into()
            }),
        }));
        assert!(app.data.loading.contains(&Target::Lyrics));
        assert_eq!(app.requests.count(), 1);
    }
}
