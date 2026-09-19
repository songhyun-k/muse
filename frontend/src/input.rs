use crate::{
    app::App,
    generated::*,
    geometry::Layout,
    requests::Target,
    state::{Dialog, EditAction, Focus, NAV, Panel, View},
    theme::THEMES,
};

impl App {
    pub fn paste(&mut self, text: &str, now: f64) {
        if let Some(editor) = &mut self.ui.editor {
            editor.insert(text);
            self.search_changed(now);
        }
    }

    fn search_changed(&mut self, now: f64) {
        if let Some(editor) = self
            .ui
            .editor
            .as_ref()
            .filter(|e| matches!(e.action, EditAction::Search))
        {
            self.ui.query = editor.buffer.clone();
            self.ui.cursors[0] = 0;
            self.invalidate_target(Target::Main);
            self.data.begin(Target::Main, 0);
            self.search_due = Some(now + 0.18);
        }
    }

    pub fn key(&mut self, key: &str, layout: Layout, now: f64, unix: f64) -> bool {
        if key == "ctrl-c" {
            return false;
        }
        if self.ui.editor.is_some() {
            match key {
                "enter" => {
                    if let Err(error) = self.commit_edit() {
                        self.ui.toast = Some((error, now));
                    }
                }
                "esc" => self.ui.editor = None,
                _ => {
                    let editor = self.ui.editor.as_mut().unwrap();
                    let before = editor.buffer.clone();
                    match key {
                        "left" => editor.move_cursor(false),
                        "right" => editor.move_cursor(true),
                        "home" => editor.edge(false),
                        "end" => editor.edge(true),
                        "clear" => editor.clear(),
                        "backspace" => editor.backspace(),
                        "delete" => editor.delete(),
                        text if text.chars().count() == 1 => editor.insert(text),
                        _ => {}
                    }
                    if before != editor.buffer {
                        self.ui.toast = None;
                        self.search_changed(now);
                    }
                }
            }
            return true;
        }
        if key == "q" {
            return false;
        }
        if self.ui.dialog.is_some() {
            let settings = matches!(self.ui.dialog, Some(Dialog::Settings { .. }));
            match key {
                "esc" => self.ui.dialog = None,
                "," if settings => self.ui.dialog = None,
                "left" if settings => self.adjust_setting(-1),
                "right" | " " if settings => self.adjust_setting(1),
                "enter" => self.choose_dialog(),
                "up" | "k" | "down" | "j" | "tab" | "backtab" => {
                    let count = self
                        .ui
                        .dialog
                        .as_ref()
                        .unwrap()
                        .rows(&self.ui, &self.data)
                        .len();
                    let dialog = self.ui.dialog.as_mut().unwrap();
                    dialog.move_by(
                        if matches!(key, "up" | "k" | "backtab") {
                            -1
                        } else {
                            1
                        },
                        count,
                    );
                }
                "M" if matches!(self.ui.dialog, Some(Dialog::Lyrics { .. })) => {
                    self.ui.dialog = None;
                    self.find_lyrics();
                }
                _ => {}
            }
            return true;
        }
        if key != "t" {
            self.ui.tour = None;
        }
        self.ui.pulse = Some((key.into(), now));
        if self.ui.help && key != "," {
            self.ui.help = false;
            return true;
        }
        match key {
            "[" => self.ui.toggle_panel(Focus::Nav, layout),
            "]" => {
                if layout.side.is_none() {
                    self.leave_full_lyrics();
                }
                self.ui.toggle_panel(Focus::Right, layout);
            }
            "tab" | "backtab" => self.ui.cycle_focus(layout, key == "backtab"),
            "I" => self.language_menu(),
            "," => self.settings_menu(),
            "l" | "Q" => {
                self.leave_full_lyrics();
                self.ui.panel = if key == "l" {
                    Panel::Lyrics
                } else {
                    Panel::Queue
                };
                self.ui.right_open = true;
                self.ui.preferred = Focus::Right;
                self.ui.focus = Focus::Right;
                self.ui.lyric_manual = None;
            }
            "up" | "k" => self.move_selection(-1, unix),
            "down" | "j" => self.move_selection(1, unix),
            "pageup" | "pagedown" => self.move_selection(
                ((layout.main.h - 4) / 2).max(1) * if key == "pageup" { -1 } else { 1 },
                unix,
            ),
            "home" => self.move_selection(-1_000_000, unix),
            "end" => self.move_selection(1_000_000, unix),
            "left" | "right" => {
                let i = NAV
                    .iter()
                    .position(|v| *v == self.ui.view)
                    .unwrap_or(self.ui.nav_cursor);
                self.go(NAV[(i + if key == "left" { NAV.len() - 1 } else { 1 }) % NAV.len()]);
            }
            "enter" if self.ui.focus == Focus::Nav => self.go(NAV[self.ui.nav_cursor]),
            "enter"
                if self.data.errors.contains_key(&Target::Main) && self.ui.focus == Focus::Main =>
            {
                self.retry()
            }
            "enter" => self.activate(),
            " " => self.control(Control::Toggle),
            "n" => self.control(Control::Next),
            "b" => self.control(Control::Previous),
            "H" => self.seek_by(-10.0, unix),
            "L" => self.seek_by(10.0, unix),
            "+" | "=" => self.volume_by(0.05),
            "-" => self.volume_by(-0.05),
            "m" => self.toggle_mute(),
            "s" => self.toggle_shuffle(),
            "r" => self.cycle_repeat(),
            "f" => self.favorite_selected(now),
            "a" => self.playlist_picker(),
            "A" => self.play_selected(Placement::Last),
            "d" if self.ui.queue_focus() => self.remove_queue(),
            "d" => self.remove_collection_song(),
            "J" | "K" if self.ui.queue_focus() => self.move_queue(key == "J"),
            "J" | "K" => self.move_collection_song(key == "J"),
            "/" => {
                let query = self.ui.query.clone();
                if self.ui.view != View::Search {
                    self.go(View::Search);
                }
                self.ui.focus = Focus::Main;
                self.edit(EditAction::Search, query);
            }
            "p" => self.go(View::Playlists),
            "o" => {
                if let Some(item) = self.selected() {
                    self.open(item.album_ref.clone().unwrap_or_else(|| item.r#ref.clone()));
                }
            }
            "R" => self.rename(),
            "N" => self.edit(EditAction::Create(None), String::new()),
            ":" | "C" => self.collection_menu(),
            "F" => self.filter_menu(),
            "M" => self.find_lyrics(),
            "O" => self.lyrics_menu(),
            "full_lyrics" => self.go(View::Lyrics),
            "play_detail" | "shuffle_detail" => self.play_detail(key == "shuffle_detail"),
            "authorize" => {
                self.send(Command::Authorize(Empty {}), Target::Mutation, 0);
            }
            "retry" => self.retry(),
            "retry_queue" => self.queue_page(0),
            "T" => self.ui.theme = (self.ui.theme + 1) % THEMES.len(),
            "1" | "2" | "3" | "4" | "5" => self.ui.theme = key.parse::<usize>().unwrap() - 1,
            "v" => self.ui.transparent = !self.ui.transparent,
            "?" => self.ui.help = true,
            "z" => {
                self.ui.reduced_motion = !self.ui.reduced_motion;
                self.ui.toast = Some((
                    if self.ui.reduced_motion {
                        "움직임 줄임"
                    } else {
                        "움직임 켬"
                    }
                    .into(),
                    now,
                ));
            }
            "t" => {
                self.ui.tour = if self.ui.tour.is_some() {
                    None
                } else {
                    Some(now)
                };
                self.ui.tour_step = 0;
                self.ui.toast = Some((
                    if self.ui.tour.is_some() {
                        "둘러보기 시작"
                    } else {
                        "둘러보기 멈춤"
                    }
                    .into(),
                    now,
                ));
            }
            "esc" => {
                while let Some((target, _)) = self.data.notification_error() {
                    let target = target.clone();
                    self.data.errors.remove(&target);
                }
                if self.ui.focus != Focus::Main {
                    self.ui.focus = Focus::Main;
                } else {
                    self.back();
                }
            }
            _ => {}
        }
        self.related();
        true
    }

    fn retry(&mut self) {
        if self
            .data
            .errors
            .get(&Target::Main)
            .is_some_and(|e| e.code == ErrorCode::NotAuthorized)
        {
            self.send(Command::Authorize(Empty {}), Target::Mutation, 0);
        } else {
            self.send(Command::Snapshot(Empty {}), Target::Bootstrap, 0);
            self.refresh(0);
        }
    }

    pub fn find_lyrics(&mut self) {
        if let Some(item) = self.data.current() {
            self.edit(
                EditAction::Lyrics(item.r#ref.clone()),
                format!("{} {}", item.artist, item.title),
            );
        }
    }

    fn rename(&mut self) {
        if let Some(collection) = self.focused_detail().and_then(|r| {
            self.data.store.as_ref().and_then(|s| {
                s.collections
                    .iter()
                    .find(|c| r.source == Source::Collection && c.id == r.id)
            })
        }) {
            self.edit(
                EditAction::Rename(collection.id.clone()),
                collection.name.clone(),
            );
        }
    }

    pub fn move_selection(&mut self, delta: i32, unix: f64) {
        if self.ui.focus == Focus::Nav {
            self.ui.nav_cursor =
                (self.ui.nav_cursor as i32 + delta).clamp(0, NAV.len() as i32 - 1) as usize;
        } else if self.ui.lyrics_focus() {
            let count = self.data.lyrics.as_ref().map_or(0, |l| l.lines.len());
            let current = self.ui.lyric_manual.unwrap_or_else(|| {
                self.data
                    .active_lyric(self.data.position(unix))
                    .unwrap_or(0)
                    .saturating_sub(1) as f64
            });
            self.ui.lyric_manual =
                Some((current + f64::from(delta)).clamp(0.0, count.saturating_sub(1) as f64));
        } else {
            let queue = self.ui.queue_focus();
            let count = if queue {
                self.data.queue.as_ref().map_or(0, |q| q.entries.len())
            } else {
                self.data.items.len()
            };
            let cursor = &mut self.ui.cursors[usize::from(queue)];
            *cursor = (*cursor as i64 + i64::from(delta)).clamp(0, count.saturating_sub(1) as i64)
                as usize;
            if delta > 0 && *cursor + 1 >= count {
                if queue {
                    if let Some(offset) = self.data.queue.as_ref().and_then(|q| q.next_offset)
                        && !self.data.loading.contains(&Target::Queue)
                    {
                        self.queue_page(offset);
                    }
                } else {
                    self.more();
                }
            }
        }
    }

    pub fn tour(&mut self, now: f64) -> bool {
        let Some(start) = self.ui.tour else {
            return false;
        };
        let step = ((now - start).max(0.0) / 5.0) as u64;
        if step == self.ui.tour_step {
            return false;
        }
        self.ui.tour_step = step;
        match step % 5 {
            0 => {
                self.ui.theme = (self.ui.theme + 1) % THEMES.len();
                if let Some(item) = self.data.current() {
                    self.open(item.album_ref.clone().unwrap_or_else(|| item.r#ref.clone()));
                } else {
                    self.go(View::Home);
                }
            }
            1 => self.go(View::Search),
            2 => self.go(View::Albums),
            3 => self.go(View::Playlists),
            _ => self.go(View::Songs),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Admission;
    #[test]
    fn reopening_sidebars_restores_the_view_and_history_reenters_full_lyrics() {
        use crate::{art::ArtCache, geometry::Visual, scene::Scene, theme::Palette};
        for key in ["Q", "l", "]"] {
            let mut app = App::default();
            let item: Item = serde_json::from_value(serde_json::json!({
                "ref":{"id":"collection","kind":"playlist","source":"collection"},
                "title":"이전 목록","artist":"","album":""
            }))
            .unwrap();
            app.ui.view = View::Playlists;
            app.detail_ref = Some(item.r#ref.clone());
            app.data.detail = Some(item.clone());
            app.go(View::Lyrics);
            app.go(View::Lyrics); // Repeated expansion must not create a self-history entry.
            assert!(!app.ui.right_open);
            let layout = Layout::new(140, 40, &app.ui);
            app.key(key, layout, 1.0, 1.0);
            assert_eq!(app.ui.view, View::Playlists);
            assert_eq!(app.detail_ref.as_ref(), Some(&item.r#ref));
            assert!(app.ui.right_open);
            assert_eq!(app.ui.focus, Focus::Right);
            let mut requests = vec![];
            app.flush(|r| {
                requests.push(r.clone());
                Ok(Admission::Accepted)
            })
            .unwrap();
            for request in requests {
                let notice = match request.command {
                    Command::Detail(p) => {
                        assert_eq!(p.item, item.r#ref);
                        Notice::Detail(Detail {
                            item: item.clone(),
                            children: vec![],
                            next_offset: None,
                        })
                    }
                    Command::Queue(_) => Notice::Queue(QueuePage {
                        entries: vec![],
                        next_offset: None,
                        revision: 0,
                        total: 0,
                    }),
                    _ => panic!("unexpected request"),
                };
                app.receive(Event {
                    version: 1,
                    id: Some(request.id),
                    sequence: request.id,
                    event: notice,
                });
            }
            let visual = Visual::settled(&app.ui, &app.data, 0.0);
            let palette = Palette::new(0, false);
            let mut art = ArtCache::default();
            let mut scene = Scene::new(&app.ui, &app.data, &visual, &palette, &mut art, 140, 40);
            scene.draw();
            let text: String = scene
                .canvas
                .buffer
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(text.contains("이전 목록"), "blank main pane after {key}");
            app.go(View::Lyrics);
            app.open(item.r#ref.clone());
            app.ui.right_open = true;
            app.back();
            assert_eq!(app.ui.view, View::Lyrics);
            assert!(!app.ui.right_open);
            assert!(app.detail_ref.is_none() && app.data.detail.is_none());
            app.back();
            assert_eq!(app.ui.view, View::Playlists);
            assert_eq!(app.detail_ref, Some(item.r#ref));
        }
    }

    #[test]
    fn actions_follow_visible_content_instead_of_hidden_collection_rows() {
        let mut app = App::default();
        let layout = Layout::new(140, 40, &app.ui);
        let song = |id: &str| {
            serde_json::from_value::<Item>(serde_json::json!({
                "ref":{"id":id,"kind":"song","source":"catalog"},
                "title":id,"artist":"a","album":"a"
            }))
            .unwrap()
        };
        let collection = ItemRef {
            id: "saved".into(),
            source: Source::Collection,
            kind: Kind::Playlist,
        };
        app.ui.view = View::Playlists;
        app.detail_ref = Some(collection.clone());
        app.data.items = vec![song("a"), song("b")];
        app.data.player = Some(
            serde_json::from_value(serde_json::json!({
                "current":song("c"),"playing":false,"position":0,"queueCount":0,
                "queueRevision":1,"updatedAt":0,"repeatMode":"off","shuffle":false,"canSeek":false
            }))
            .unwrap(),
        );
        app.data.lyrics = Some(Lyrics {
            item: song("c").r#ref,
            match_id: None,
            status: LyricsStatus::Missing,
            lines: vec![],
            offset: 0.0,
        });
        for focus in [Focus::Nav, Focus::Right] {
            app.ui.focus = focus;
            for key in ["d", "J", "K", "R"] {
                app.key(key, layout, 1.0, 1.0);
            }
            assert_eq!(app.flush(|_| panic!("hidden collection edit")).unwrap(), 0);
            assert!(app.ui.editor.is_none());
        }
        app.key("full_lyrics", layout, 1.0, 1.0);
        app.ui.lyric_manual = Some(5.0);
        for key in ["d", "J", "K", "R", "enter"] {
            app.key(key, layout, 1.0, 1.0);
        }
        assert_eq!(app.ui.lyric_manual, None);
        assert!(app.detail_ref.is_none() && app.data.items.is_empty());
        app.key("f", layout, 1.0, 1.0);
        app.key("A", layout, 1.0, 1.0);
        let mut commands = vec![];
        app.flush(|r| {
            commands.push(r.command.clone());
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert_eq!(commands.len(), 2);
        assert!(matches!(&commands[0], Command::Favorite(p) if p.item.id == "c"));
        assert!(matches!(&commands[1], Command::Play(p) if p.items[0].id == "c"));
        app.key("a", layout, 1.0, 1.0);
        let rows = app.ui.dialog.as_ref().unwrap().rows(&app.ui, &app.data);
        assert!(
            matches!(&rows[0].choice, crate::state::Choice::Edit(EditAction::Create(Some(item)), _) if item.id == "c")
        );
        app.ui.dialog = None;
        app.back();
        assert_eq!(app.detail_ref, Some(collection));
        assert_eq!(app.ui.view, View::Playlists);
    }

    #[test]
    fn korean_search_debounces_and_panel_focus_keeps_independent_cursors() {
        let mut app = App::default();
        let layout = Layout::new(140, 40, &app.ui);
        for key in ["/", "유", "나"] {
            assert!(app.key(key, layout, 1.0, 1000.0));
        }
        app.dispatch_due(1.1);
        assert_eq!(app.flush(|_| Ok(Admission::Accepted)).unwrap(), 0);
        app.dispatch_due(1.2);
        assert_eq!(
            app.flush(|r| {
                assert!(matches!(&r.command, Command::Search(p) if p.query == "유나"));
                Ok(Admission::Accepted)
            })
            .unwrap(),
            1
        );
        app.key("esc", layout, 2.0, 1000.0);
        app.ui.cursors = [5, 2];
        app.key("Q", layout, 2.0, 1000.0);
        app.key("]", layout, 2.0, 1000.0);
        assert_eq!(app.ui.focus, Focus::Main);
        assert_eq!(app.ui.cursors, [5, 2]);
        assert!(!app.key("q", layout, 2.0, 1000.0));
        app.go(View::Artists);
        let artist = ItemRef {
            id: "artist".into(),
            source: Source::Library,
            kind: Kind::Artist,
        };
        app.open(artist.clone());
        app.open(ItemRef {
            id: "album".into(),
            source: Source::Library,
            kind: Kind::Album,
        });
        app.back();
        assert_eq!(app.detail_ref, Some(artist));
        app.back();
        assert_eq!(app.ui.view, View::Artists);
    }

    #[test]
    fn completed_creation_does_not_override_later_navigation() {
        let mut app = App::default();
        app.edit(EditAction::Create(None), "Night".into());
        app.commit_edit().unwrap();
        let mut id = 0;
        app.flush(|r| {
            id = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        app.go(View::Favorites);
        let collection = Collection {
            id: "new".into(),
            name: "Night".into(),
            description: String::new(),
            count: 0,
        };
        app.receive(Event {
            version: 1,
            id: Some(id),
            sequence: 1,
            event: Notice::CollectionCreated(CreatedCollection {
                collection: collection.clone(),
                store: StoreState {
                    collections: vec![collection],
                    favorites: vec![],
                    history_count: 0,
                },
            }),
        });
        assert_eq!(app.ui.view, View::Favorites);
        assert!(app.detail_ref.is_none());
        assert_eq!(app.data.store.unwrap().collections.len(), 1);
    }
    #[test]
    fn tour_changes_views_at_five_seconds_without_playback_commands() {
        let mut app = App::default();
        let layout = Layout::new(140, 40, &app.ui);
        app.key("t", layout, 1.0, 1000.0);
        assert!(!app.tour(5.9));
        assert!(app.tour(6.0));
        assert_eq!(app.ui.view, View::Search);
        assert!(!app.tour(6.1));
        assert!(app.tour(11.0));
        assert_eq!(app.ui.view, View::Albums);
        app.flush(|request| {
            assert!(matches!(request.command, Command::Browse(_)));
            Ok(Admission::Accepted)
        })
        .unwrap();
        app.key("j", layout, 12.0, 1000.0);
        assert!(!app.tour(20.0));
    }
}
