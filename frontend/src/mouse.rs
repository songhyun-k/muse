use crate::{
    app::App,
    generated::{ItemRef, Kind, Source},
    geometry::{Area, Layout},
    scene::{Action, Hit},
    state::Focus,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

enum Drag {
    Seek(Area, String),
    Volume(Area),
}

#[derive(Default)]
pub struct Mouse {
    drag: Option<Drag>,
    last_click: Option<(String, f64)>,
}

impl Mouse {
    pub fn reset(&mut self) {
        self.drag = None;
        self.last_click = None;
    }

    pub fn handle(
        &mut self,
        app: &mut App,
        event: MouseEvent,
        layout: Layout,
        hits: &[Hit],
        now: f64,
        unix: f64,
    ) -> bool {
        let point = (i32::from(event.column), i32::from(event.row));
        if app.ui.hover != Some(point) {
            app.ui.hover = Some(point);
            app.ui.hover_since = now;
        }
        match event.kind {
            MouseEventKind::Up(_) => {
                self.drag = None;
                return true;
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.slide(app, point.0);
                return true;
            }
            MouseEventKind::Moved | MouseEventKind::Drag(_) => return true,
            _ => {}
        }
        app.ui.tour = None;
        if app.ui.help {
            if event.kind == MouseEventKind::Down(MouseButton::Left) {
                if hits
                    .iter()
                    .rev()
                    .find(|hit| hit.area.contains(point))
                    .is_some_and(|hit| matches!(hit.action, Action::Key(",")))
                {
                    return app.key(",", layout, now, unix);
                }
                app.ui.help = false;
            }
            return true;
        }
        let scroll = match event.kind {
            MouseEventKind::ScrollUp => -1,
            MouseEventKind::ScrollDown => 1,
            _ => 0,
        };
        if app.ui.dialog.is_none() && app.ui.editor.is_none() {
            for (focus, area) in [
                (Focus::Main, Some(layout.main)),
                (Focus::Nav, layout.nav),
                (Focus::Right, layout.side),
            ] {
                if area.is_some_and(|a| a.contains(point)) {
                    app.ui.focus = focus;
                }
            }
        }
        if scroll != 0 {
            self.reset();
            if let Some(dialog) = &app.ui.dialog {
                let count = dialog.rows(&app.ui, &app.data).len();
                app.ui.dialog.as_mut().unwrap().move_by(scroll, count);
            } else if app.ui.editor.is_none() {
                app.move_selection(scroll, unix);
            }
            return true;
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return true;
        }
        let Some(hit) = hits.iter().rev().find(|hit| hit.area.contains(point)) else {
            return true;
        };
        if app.ui.dialog.is_some()
            && !matches!(hit.action, Action::Dialog(_) | Action::Key("M" | "esc"))
        {
            return true;
        }
        app.ui.editor = None;
        self.drag = None;
        if !matches!(hit.action, Action::Select { .. }) {
            self.last_click = None;
        }
        match &hit.action {
            Action::Key(key) => return app.key(key, layout, now, unix),
            Action::Go(view) => app.go(*view),
            Action::Collection(id) => app.open(ItemRef {
                id: id.clone(),
                kind: Kind::Playlist,
                source: Source::Collection,
            }),
            Action::Open(item) => app.open(item.clone()),
            Action::Select { index, queue, key } => {
                let Some(identity) = app
                    .data
                    .row_key(*index, *queue)
                    .filter(|identity| identity == key)
                else {
                    self.last_click = None;
                    return true;
                };
                app.ui.focus = if *queue { Focus::Right } else { Focus::Main };
                app.ui.cursors[usize::from(*queue)] = *index;
                let identity = if *queue {
                    format!("queue:{identity}")
                } else {
                    format!("{}:{index}:{identity}", app.navigation_revision)
                };
                if self
                    .last_click
                    .as_ref()
                    .is_some_and(|(previous, since)| previous == &identity && now - since < 0.4)
                {
                    app.activate();
                    self.last_click = None;
                } else {
                    self.last_click = Some((identity, now));
                }
            }
            Action::Favorite(item) => app.favorite_at(item.clone(), now),
            Action::Seek => {
                self.drag = app
                    .data
                    .player
                    .as_ref()
                    .and_then(|p| p.current_entry_id.clone())
                    .map(|entry| Drag::Seek(hit.area, entry));
                self.slide(app, point.0);
            }
            Action::Volume => {
                self.drag = Some(Drag::Volume(hit.area));
                self.slide(app, point.0);
            }
            Action::Dialog(index) => {
                if let Some(dialog) = &app.ui.dialog {
                    let count = dialog.rows(&app.ui, &app.data).len();
                    let dialog = app.ui.dialog.as_mut().unwrap();
                    dialog.move_by(*index as i32 - dialog.cursor() as i32, count);
                    app.choose_dialog();
                }
            }
        }
        true
    }

    fn slide(&mut self, app: &mut App, x: i32) {
        let ratio =
            |area: Area| (f64::from(x - area.x) / f64::from((area.w - 1).max(1))).clamp(0.0, 1.0);
        match &self.drag {
            Some(Drag::Volume(area)) => app.volume_to(ratio(*area)),
            Some(Drag::Seek(area, entry))
                if app
                    .data
                    .player
                    .as_ref()
                    .and_then(|p| p.current_entry_id.as_ref())
                    == Some(entry) =>
            {
                if let Some(duration) = app.data.current().and_then(|i| i.duration) {
                    app.seek(ratio(*area) * duration);
                }
            }
            _ => self.drag = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generated::*,
        state::{Choice, Dialog, MenuRow},
        transport::Admission,
    };
    use crossterm::event::KeyModifiers;

    #[test]
    fn animated_hits_wait_until_their_visible_content_changes() {
        use crate::{
            art::ArtCache,
            effects::Effects,
            geometry::Visual,
            scene::Scene,
            state::{Panel, View},
            theme::Palette,
        };
        let paint = |app: &App, effects: &mut Effects, now: f64| {
            let mut visual = Visual::settled(&app.ui, &app.data, 1000.0);
            visual.now = now;
            let palette = Palette::new(app.ui.theme, app.ui.transparent);
            let mut art = ArtCache::default();
            let mut scene = Scene::new(&app.ui, &app.data, &visual, &palette, &mut art, 140, 40);
            scene.draw_animated(effects);
            let text: String = scene
                .canvas
                .buffer
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            (scene.layout, scene.hits, text)
        };
        for reduced in [false, true] {
            for (view, queue, change_view) in [
                (View::Albums, false, false),
                (View::Songs, false, false),
                (View::Songs, true, false),
                (View::Songs, false, true),
            ] {
                let mut app = App::default();
                app.ui.view = view;
                app.ui.panel = Panel::Queue;
                app.ui.reduced_motion = reduced;
                let visible: Item = serde_json::from_value(serde_json::json!({
                    "ref":{"id":"old","source":"library","kind":if view == View::Albums { "album" } else { "song" }},
                    "title":"Visible A","artist":"a","album":"a"
                })).unwrap();
                app.data.items = vec![visible.clone()];
                app.data.queue = Some(QueuePage {
                    entries: vec![QueueEntry {
                        id: "old-entry".into(),
                        item: Some(visible.clone()),
                    }],
                    next_offset: None,
                    revision: 1,
                    total: 1,
                });
                let mut effects = Effects::default();
                paint(&app, &mut effects, 1.0);
                let (_, hits, text) = paint(&app, &mut effects, 1.5);
                assert!(text.contains("Visible A"));
                let area = hits
                    .iter()
                    .find(|hit| match &hit.action {
                        Action::Open(_) => view == View::Albums,
                        Action::Select { queue: q, .. } => view != View::Albums && *q == queue,
                        _ => false,
                    })
                    .unwrap()
                    .area;
                let mut next = visible;
                next.r#ref.id = "new".into();
                next.title = "Visible B".into();
                app.data.items = vec![next.clone()];
                app.data.queue.as_mut().unwrap().entries = vec![QueueEntry {
                    id: "new-entry".into(),
                    item: Some(next),
                }];
                if change_view {
                    app.ui.view = View::History;
                } else {
                    app.ui.theme = 1;
                }
                effects.observe(&app.ui, &app.data, 2.0);
                let mut mouse = Mouse::default();
                for now in [2.01, 2.24] {
                    let (layout, hits, text) = paint(&app, &mut effects, now);
                    let revealed = reduced || now > 2.2;
                    assert!(
                        text.contains(if revealed { "Visible B" } else { "Visible A" }),
                        "missing title: {view:?}, queue={queue}, change_view={change_view}, reduced={reduced}, now={now}"
                    );
                    for time in [now, now + 0.01] {
                        mouse.handle(
                            &mut app,
                            MouseEvent {
                                kind: MouseEventKind::Down(MouseButton::Left),
                                column: (area.x + 2) as u16,
                                row: area.y as u16,
                                modifiers: KeyModifiers::NONE,
                            },
                            layout,
                            &hits,
                            time,
                            1000.0,
                        );
                    }
                    assert_eq!(app.detail_ref.is_some(), revealed && view == View::Albums);
                    let count = app
                        .flush(|r| {
                            assert!(revealed, "old glyphs activated a new item");
                            match &r.command {
                                Command::Detail(p) => assert_eq!(p.item.id, "new"),
                                Command::Play(p) => assert_eq!(p.items[0].id, "new"),
                                Command::QueueJump(p) => assert_eq!(p.entry_id, "new-entry"),
                                other => panic!("unexpected command: {other:?}"),
                            }
                            Ok(Admission::Accepted)
                        })
                        .unwrap();
                    assert_eq!(count, usize::from(revealed));
                    // Reduced motion activates immediately; the later frame is the opened detail.
                    if reduced {
                        break;
                    }
                }
            }
        }
    }

    #[test]
    fn reopening_current_detail_restores_focus_without_resetting_selection_or_history() {
        use crate::{
            art::ArtCache,
            geometry::Visual,
            scene::Scene,
            state::{Panel, View},
            theme::Palette,
        };
        for source in [Source::Collection, Source::Library] {
            let mut app = App::default();
            app.ui.view = View::Albums;
            let reference = ItemRef {
                id: "shown".into(),
                source,
                kind: if source == Source::Collection {
                    Kind::Playlist
                } else {
                    Kind::Album
                },
            };
            app.open(reference.clone());
            app.ui.cursors[0] = 4;
            let revision = app.navigation_revision;
            if source == Source::Collection {
                app.data.store = Some(StoreState {
                    collections: vec![Collection {
                        id: reference.id.clone(),
                        name: "Shown".into(),
                        description: String::new(),
                        count: 5,
                    }],
                    favorites: vec![],
                    history_count: 0,
                });
                let visual = Visual::settled(&app.ui, &app.data, 1000.0);
                let palette = Palette::new(0, false);
                let mut art = ArtCache::default();
                let mut scene =
                    Scene::new(&app.ui, &app.data, &visual, &palette, &mut art, 140, 40);
                scene.draw();
                let area = scene
                    .hits
                    .iter()
                    .find(|h| matches!(h.action, Action::Collection(_)))
                    .unwrap()
                    .area;
                let (layout, hits) = (scene.layout, scene.hits);
                Mouse::default().handle(
                    &mut app,
                    MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: (area.x + 1) as u16,
                        row: area.y as u16,
                        modifiers: KeyModifiers::NONE,
                    },
                    layout,
                    &hits,
                    1.0,
                    1000.0,
                );
            } else {
                let item: Item = serde_json::from_value(serde_json::json!({
                    "ref":{"id":"song","source":"library","kind":"song"},
                    "title":"Song","artist":"a","album":"a","albumRef":reference
                }))
                .unwrap();
                app.data.queue = Some(QueuePage {
                    entries: vec![QueueEntry {
                        id: "entry".into(),
                        item: Some(item),
                    }],
                    next_offset: None,
                    revision: 1,
                    total: 1,
                });
                app.ui.panel = Panel::Queue;
                app.ui.focus = Focus::Right;
                let layout = Layout::new(140, 40, &app.ui);
                app.key("o", layout, 1.0, 1000.0);
            }
            assert_eq!(
                app.ui.focus,
                Focus::Main,
                "same-detail selection lost focus"
            );
            assert_eq!(app.ui.cursors[0], 4);
            assert_eq!(app.navigation_revision, revision);
            app.back();
            assert_eq!(app.ui.view, View::Albums);
            assert!(app.detail_ref.is_none(), "same detail was added to history");
        }
    }

    #[test]
    fn stale_card_hit_does_not_open_an_album_that_was_never_rendered() {
        use crate::{art::ArtCache, geometry::Visual, scene::Scene, state::View, theme::Palette};
        let mut app = App::default();
        app.ui.view = View::Albums;
        let visible: Item = serde_json::from_value(serde_json::json!({
            "ref":{"id":"visible-album","source":"library","kind":"album"},
            "title":"Visible album","artist":"a","album":"a"
        }))
        .unwrap();
        app.data.items = vec![visible.clone()];
        let visual = Visual::settled(&app.ui, &app.data, 1000.0);
        let palette = Palette::new(0, false);
        let mut art = ArtCache::default();
        let mut scene = Scene::new(&app.ui, &app.data, &visual, &palette, &mut art, 140, 40);
        scene.draw();
        let layout = scene.layout;
        let hits = scene.hits;
        let area = hits
            .iter()
            .find(|h| matches!(h.action, Action::Open(_)))
            .unwrap()
            .area;
        app.refresh(0);
        let mut id = 0;
        app.flush(|r| {
            id = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        let mut replacement = visible.clone();
        replacement.r#ref.id = "not-yet-rendered-album".into();
        app.receive(Event {
            version: 1,
            id: Some(id),
            sequence: 1,
            event: Notice::Page(Page {
                items: vec![replacement],
                next_offset: None,
            }),
        });
        Mouse::default().handle(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: (area.x + 1) as u16,
                row: area.y as u16,
                modifiers: KeyModifiers::NONE,
            },
            layout,
            &hits,
            1.0,
            1000.0,
        );
        assert_eq!(
            app.detail_ref,
            Some(visible.r#ref),
            "opened an album from an unpainted page"
        );
        Mouse::default().handle(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: (area.x + 1) as u16,
                row: area.y as u16,
                modifiers: KeyModifiers::NONE,
            },
            layout,
            &hits,
            1.1,
            1000.0,
        );
        app.back();
        assert_eq!(
            app.ui.view,
            View::Albums,
            "double click added a duplicate history location"
        );
        assert!(app.detail_ref.is_none());
    }

    #[test]
    fn stale_row_hits_cannot_play_replacement_songs_or_queue_entries() {
        use crate::{
            art::ArtCache, geometry::Visual, requests::Target, scene::Scene, state::Panel,
            theme::Palette,
        };
        let painted = |app: &App| {
            let visual = Visual::settled(&app.ui, &app.data, 1000.0);
            let palette = Palette::new(0, false);
            let mut art = ArtCache::default();
            let mut scene = Scene::new(&app.ui, &app.data, &visual, &palette, &mut art, 140, 40);
            scene.draw();
            (scene.layout, scene.hits)
        };
        for queue in [false, true] {
            let mut app = App::default();
            if queue {
                app.ui.panel = Panel::Queue;
            }
            let item: Item = serde_json::from_value(serde_json::json!({
                "ref":{"id":"visible-song","source":"library","kind":"song"},
                "title":"Visible song","artist":"a","album":"a"
            }))
            .unwrap();
            app.data.items = vec![item.clone()];
            app.data.queue = Some(QueuePage {
                entries: vec![QueueEntry {
                    id: "old-entry".into(),
                    item: Some(item.clone()),
                }],
                next_offset: None,
                revision: 1,
                total: 1,
            });
            let (layout, old_hits) = painted(&app);
            let target = if queue { Target::Queue } else { Target::Main };
            let request = if queue {
                Command::Queue(QueueParams {
                    offset: 0,
                    revision: None,
                })
            } else {
                Command::Browse(BrowseParams {
                    scope: Scope::Library,
                    kind: Kind::Song,
                    collection_id: None,
                    order: Order::Recent,
                    offset: 0,
                })
            };
            app.send(request, target, 0);
            let mut id = 0;
            app.flush(|r| {
                id = r.id;
                Ok(Admission::Accepted)
            })
            .unwrap();
            let notice = if queue {
                Notice::Queue(QueuePage {
                    entries: vec![QueueEntry {
                        id: "new-entry".into(),
                        item: None,
                    }],
                    next_offset: None,
                    revision: 2,
                    total: 1,
                })
            } else {
                let mut next = item;
                next.r#ref.id = "new-song".into();
                Notice::Page(Page {
                    items: vec![next],
                    next_offset: None,
                })
            };
            app.receive(Event {
                version: 1,
                id: Some(id),
                sequence: 1,
                event: notice,
            });
            let mut mouse = Mouse::default();
            for (fresh, hits) in [(false, old_hits), (true, painted(&app).1)] {
                let area = hits
                    .iter()
                    .find(|h| matches!(h.action, Action::Select { queue: q, .. } if q == queue))
                    .unwrap()
                    .area;
                for now in [1.0, 1.1] {
                    mouse.handle(
                        &mut app,
                        MouseEvent {
                            kind: MouseEventKind::Down(MouseButton::Left),
                            column: (area.x + 2) as u16,
                            row: area.y as u16,
                            modifiers: KeyModifiers::NONE,
                        },
                        layout,
                        &hits,
                        now,
                        1000.0,
                    );
                }
                let count = app.flush(|r| {
                    assert!(fresh, "stale row activated a replacement item");
                    if queue { assert!(matches!(&r.command, Command::QueueJump(p) if p.entry_id == "new-entry")); }
                    else { assert!(matches!(&r.command, Command::Play(p) if p.items[0].id == "new-song")); }
                    Ok(Admission::Accepted)
                }).unwrap();
                assert_eq!(count, usize::from(fresh));
            }
        }
    }

    #[test]
    fn pointer_uses_topmost_hits_modal_rows_and_coalesced_drags() {
        let mut app = App::default();
        let mut mouse = Mouse::default();
        let layout = Layout::new(140, 40, &app.ui);
        let area = Area::new(30, 10, 11, 1);
        let hits = [
            Hit {
                area,
                action: Action::Key("q"),
            },
            Hit {
                area,
                action: Action::Volume,
            },
        ];
        app.data.volume = Some(VolumeState {
            device: "test".into(),
            level: Some(0.5),
            muted: Some(false),
            can_set_volume: true,
            can_mute: true,
        });
        let event = |kind, x| MouseEvent {
            kind,
            column: x,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        assert!(mouse.handle(
            &mut app,
            event(MouseEventKind::Down(MouseButton::Left), 30),
            layout,
            &hits,
            0.0,
            1000.0
        ));
        mouse.handle(
            &mut app,
            event(MouseEventKind::Drag(MouseButton::Left), 90),
            layout,
            &hits,
            0.1,
            1000.0,
        );
        mouse.handle(
            &mut app,
            event(MouseEventKind::Up(MouseButton::Left), 90),
            layout,
            &hits,
            0.2,
            1000.0,
        );
        mouse.handle(
            &mut app,
            event(MouseEventKind::Drag(MouseButton::Left), 30),
            layout,
            &hits,
            0.3,
            1000.0,
        );
        assert_eq!(
            app.flush(|r| {
                assert!(matches!(&r.command, Command::Volume(v) if v.level == Some(1.0)));
                Ok(Admission::Accepted)
            })
            .unwrap(),
            1
        );
        assert_eq!(app.data.volume.as_ref().unwrap().level, Some(0.5));
        app.ui.dialog = Some(Dialog::Menu {
            title: "test".into(),
            rows: vec![MenuRow {
                label: "close".into(),
                detail: "".into(),
                checked: false,
                choice: Choice::Close,
            }],
            cursor: 0,
        });
        let modal = [Hit {
            area,
            action: Action::Dialog(0),
        }];
        mouse.handle(
            &mut app,
            event(MouseEventKind::Down(MouseButton::Left), 35),
            layout,
            &modal,
            0.4,
            1000.0,
        );
        assert!(app.ui.dialog.is_none());
        app.ui.help = true;
        mouse.handle(
            &mut app,
            event(MouseEventKind::Down(MouseButton::Left), 35),
            layout,
            &hits,
            0.5,
            1000.0,
        );
        assert!(!app.ui.help);
        assert!(mouse.drag.is_none());
    }
}
