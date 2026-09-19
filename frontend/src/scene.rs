use crate::{
    art::ArtCache,
    canvas::Canvas,
    geometry::{Area, Layout, Visual},
    icons::Icon,
    state::{Data, Focus, NAV, Panel, Ui, View, item_key},
    theme::{Ink, Palette},
};

#[derive(Clone, Debug)]
pub enum Action {
    Key(&'static str),
    Go(View),
    Collection(String),
    Open(crate::generated::ItemRef),
    Select {
        index: usize,
        queue: bool,
        key: String,
    },
    Favorite(crate::generated::ItemRef),
    Seek,
    Volume,
    Dialog(usize),
}

pub struct Hit {
    pub area: Area,
    pub action: Action,
}

pub struct Scene<'a> {
    pub ui: &'a Ui,
    pub data: &'a Data,
    pub visual: &'a Visual,
    pub art_cache: &'a mut ArtCache,
    pub canvas: Canvas<'a>,
    pub layout: Layout,
    pub hits: Vec<Hit>,
    pub artwork: Vec<crate::generated::Item>,
}

impl<'a> Scene<'a> {
    pub fn new(
        ui: &'a Ui,
        data: &'a Data,
        visual: &'a Visual,
        palette: &'a Palette,
        art_cache: &'a mut ArtCache,
        width: u16,
        height: u16,
    ) -> Self {
        let layout = visual.panel_widths.map_or_else(
            || Layout::new(width.into(), height.into(), ui),
            |sizes| Layout::animated(width.into(), height.into(), sizes),
        );
        for item in data.current().into_iter().chain(data.items.iter()) {
            art_cache.seed(item);
        }
        Self {
            ui,
            data,
            visual,
            art_cache,
            canvas: Canvas::new(width, height, palette),
            layout,
            hits: Vec::new(),
            artwork: Vec::new(),
        }
    }

    pub fn icon(&self, icon: Icon) -> &'static str {
        icon.glyph(self.ui.plain_icons)
    }

    pub fn art(&mut self, area: Area, item: &crate::generated::Item) {
        if (2..=96).contains(&area.w)
            && (1..=48).contains(&area.h)
            && area.x < self.canvas.width()
            && area.x + area.w > 0
            && area.y < self.canvas.height()
            && area.y + area.h > 0
            && item.artwork_url.is_some()
        {
            self.artwork.push(item.clone());
        }
        let image = self.data.artwork.get(&item_key(&item.r#ref)).or_else(|| {
            item.album_ref
                .as_ref()
                .and_then(|r| self.data.artwork.get(&item_key(r)))
        });
        self.art_cache.paint(&mut self.canvas, area, item, image);
    }

    pub fn blended_art(&mut self, area: Area, item: &crate::generated::Item) {
        let previous = self
            .visual
            .previous_track
            .clone()
            .filter(|_| self.visual.cover_blend < 1.0);
        let Some(previous) = previous else {
            self.art(area, item);
            return;
        };
        self.art(area, &previous);
        let coordinates: Vec<_> = (area.y..area.y + area.h)
            .flat_map(|y| (area.x..area.x + area.w).map(move |x| (x as u16, y as u16)))
            .filter(|&(x, y)| self.canvas.buffer.area.contains((x, y).into()))
            .collect();
        let colors: Vec<_> = coordinates
            .iter()
            .map(|&point| {
                let cell = &self.canvas.buffer[point];
                (cell.fg, cell.bg)
            })
            .collect();
        self.art(area, item);
        for (point, (fg, bg)) in coordinates.into_iter().zip(colors) {
            let cell = &mut self.canvas.buffer[point];
            cell.fg = crate::effects::mix_color(fg, cell.fg, self.visual.cover_blend);
            cell.bg = crate::effects::mix_color(bg, cell.bg, self.visual.cover_blend);
        }
    }

    pub fn hovered(&self, area: Area) -> bool {
        self.ui.dialog.is_none() && self.ui.hover.is_some_and(|p| area.contains(p))
    }

    fn focused(&self, pane: Focus) -> bool {
        self.ui.focus == pane
            && !self.ui.help
            && self.ui.dialog.is_none()
            && self.ui.editor.is_none()
    }

    pub fn hit(&mut self, area: Area, action: Action) {
        let right = (area.x + area.w).min(self.canvas.width());
        let bottom = (area.y + area.h).min(self.canvas.height());
        let x = area.x.max(0);
        let y = area.y.max(0);
        if right > x && bottom > y {
            self.hits.push(Hit {
                area: Area::new(x, y, right - x, bottom - y),
                action,
            });
        }
    }

    pub fn capsule(&mut self, area: Area, bg: Ink, fg: Ink, text: &str, bold: bool) {
        let Area { x, y, w, .. } = area;
        if bg != Ink::Default && !self.ui.plain_icons && w >= 3 {
            self.canvas.text(x, y, self.icon(Icon::CapLeft), bg);
            self.canvas.fill(x + 1, y, w - 2, 1, bg);
            self.canvas
                .text(x + w - 1, y, self.icon(Icon::CapRight), bg);
        } else {
            self.canvas.fill(x, y, w, 1, bg);
        }
        self.canvas.label(x + 1, y, text, fg, bold, w - 2);
    }

    pub fn icon_button(&mut self, x: i32, y: i32, icon: Icon, key: &'static str, active: bool) {
        let p = self.canvas.palette;
        let area = Area::new(x - 1, y, 3, 1);
        let hover = self.hovered(area);
        let mut color = p[if active || hover {
            "accent"
        } else {
            "text.secondary"
        }];
        if !self.ui.reduced_motion
            && let Some((pulse, since)) = &self.ui.pulse
            && pulse == key
            && self.visual.now - since < 0.35
        {
            color = p["fx.highlight"].mix(
                p["accent"],
                crate::geometry::ease((self.visual.now - since) / 0.35),
            );
        }
        if hover {
            self.capsule(area, p["control.hover"], color, self.icon(icon), false);
        } else {
            self.canvas.text(x, y, self.icon(icon), color);
        }
        self.hit(area, Action::Key(key));
    }

    /// Returns false for the dedicated small-terminal view.
    pub fn chrome(&mut self) -> bool {
        let p = self.canvas.palette;
        let width = self.canvas.width();
        let height = self.canvas.height();
        if width < 80 || height < 24 {
            self.canvas.label(
                2,
                2,
                self.ui.text("80×24 이상으로 넓혀주세요"),
                p["accent"],
                false,
                width - 4,
            );
            self.canvas.label(
                2,
                4,
                &format!(
                    "{} {width}×{height} · q {}",
                    self.ui.text("현재"),
                    self.ui.text("종료")
                ),
                p["text.secondary"],
                false,
                width - 4,
            );
            return false;
        }
        for area in [self.layout.nav, self.layout.side].into_iter().flatten() {
            self.canvas
                .fill(area.x, 0, area.w, height - 5, p["panel.background"]);
        }
        if let Some(nav) = self.layout.nav {
            self.navigation(nav);
        } else {
            self.icon_button(2, 1, Icon::Sidebar, "[", false);
        }
        if let Some(side) = self.layout.side {
            self.side_header(side);
        } else {
            self.icon_button(width - 3, 1, Icon::ClosePanel, "]", false);
        }
        let Area { x, w, .. } = self.layout.main;
        let catalog = self
            .data
            .detail
            .as_ref()
            .is_some_and(|i| i.r#ref.source == crate::generated::Source::Catalog);
        let crumb = if !catalog
            && matches!(
                self.ui.view,
                View::Songs | View::Albums | View::Artists | View::Recent | View::Detail
            ) {
            self.ui.text("보관함")
        } else {
            "Music"
        };
        self.canvas.label(
            x + 1,
            1,
            &format!("{crumb}  {}", self.icon(Icon::Chevron)),
            p[if self.focused(Focus::Main) {
                "focus"
            } else {
                "text.secondary"
            }],
            self.focused(Focus::Main),
            (w - 24).max(5),
        );
        let controls = x + w - 4 - if self.layout.side.is_none() { 4 } else { 0 };
        self.icon_button(controls - 8, 1, Icon::Search, "/", false);
        self.icon_button(controls - 4, 1, Icon::Theme, "T", false);
        self.icon_button(controls, 1, Icon::Transparent, "v", self.ui.transparent);
        let active = match self.ui.focus {
            Focus::Nav => self.layout.nav,
            Focus::Main => Some(self.layout.main),
            Focus::Right => self.layout.side,
        };
        if self.focused(self.ui.focus)
            && let Some(area) = active
        {
            self.canvas.label(
                area.x + 1,
                2,
                &"━".repeat((area.w - 2).max(0) as usize),
                p["focus"],
                true,
                area.w - 2,
            );
        }
        true
    }

    fn navigation(&mut self, area: Area) {
        let Area { x, y, w, h } = area;
        let p = self.canvas.palette;
        let focused = self.focused(Focus::Nav);
        let heading = p[if focused { "focus" } else { "text.secondary" }];
        let active = if NAV.contains(&self.ui.view) {
            self.ui.view
        } else {
            self.ui.back
        };
        self.canvas
            .text(x + 3, 1, self.icon(Icon::Headphones), heading);
        self.canvas
            .label(x + 6, 1, "MUSIC", heading, focused, w - 6);
        self.icon_button(x + w - 3, 1, Icon::ClosePanel, "[", false);
        let icons = [
            Icon::Search,
            Icon::Home,
            Icon::Clock,
            Icon::Artist,
            Icon::Album,
            Icon::Songs,
            Icon::Playlist,
            Icon::HeartEmpty,
            Icon::History,
        ];
        let compact = h < 20;
        let mut yy = y;
        for (i, (view, icon)) in NAV.into_iter().zip(icons).enumerate() {
            if i == 2 || i == 6 {
                self.canvas.label(
                    x + 3,
                    yy + i32::from(!compact),
                    if i == 2 {
                        self.ui.text("보관함")
                    } else {
                        self.ui.text("플레이리스트")
                    },
                    p["text.quiet"],
                    false,
                    w - 6,
                );
                yy += if compact { 1 } else { 3 };
            }
            if yy >= y + h {
                break;
            }
            let row = Area::new(x + 1, yy, w - 2, 1);
            let hover = self.hovered(row);
            let strength = (-(i as f64 - self.visual.nav_position).powi(2) * 3.0).exp();
            let bg = if hover {
                p["control.hover"]
            } else {
                p["panel.background"].mix(p["nav.cursor.background"], strength)
            };
            self.capsule(row, bg, p["text.primary"], "", false);
            let color = p[if active == view {
                "nav.active"
            } else if hover {
                "text.primary"
            } else {
                "text.secondary"
            }];
            self.canvas.text(x + 3, yy, self.icon(icon), color);
            self.canvas.label(
                x + 6,
                yy,
                self.ui.text(view.label()),
                p[if active == view {
                    "nav.active"
                } else {
                    "text.primary"
                }],
                false,
                w - 7,
            );
            if focused && i == self.ui.nav_cursor {
                self.canvas.text(x, yy, "▏", p["focus"]);
            }
            self.hit(row, Action::Go(view));
            yy += 1;
        }
        if yy + 2 < y + h
            && let Some(collection) = self.data.store.as_ref().and_then(|s| s.collections.first())
        {
            self.canvas.text(
                x + 3,
                yy + 2,
                self.icon(Icon::Playlist),
                p["accent.secondary"],
            );
            self.canvas.label(
                x + 6,
                yy + 2,
                &collection.name,
                p["text.secondary"],
                false,
                w - 7,
            );
            self.hit(
                Area::new(x + 1, yy + 2, w - 2, 1),
                Action::Collection(collection.id.clone()),
            );
        }
    }

    fn side_header(&mut self, area: Area) {
        let p = self.canvas.palette;
        let lyrics = self.ui.panel == Panel::Lyrics;
        let focused = self.focused(Focus::Right);
        let heading = p[if focused { "focus" } else { "text.secondary" }];
        self.canvas.text(
            area.x + 3,
            1,
            self.icon(if lyrics { Icon::Lyrics } else { Icon::Playlist }),
            heading,
        );
        self.canvas.label(
            area.x + 6,
            1,
            if lyrics {
                self.ui.text("가사")
            } else {
                self.ui.text("재생 큐")
            },
            heading,
            focused,
            area.w - 14,
        );
        self.icon_button(
            area.x + area.w - 7,
            1,
            if lyrics { Icon::Playlist } else { Icon::Lyrics },
            if lyrics { "Q" } else { "l" },
            false,
        );
        self.icon_button(area.x + area.w - 3, 1, Icon::OpenPanel, "]", false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_emphasis_follows_keyboard_focus_without_painting_terminal_backgrounds() {
        use crate::{
            app::App,
            state::{Dialog, EditAction, Editor},
            theme::THEMES,
        };
        use ratatui::style::{Color, Modifier};

        let render = |ui: &Ui, width: u16, height: u16| {
            let data = Data::default();
            let visual = Visual::settled(ui, &data, 1000.0);
            let palette = Palette::new(ui.theme, ui.transparent);
            let mut cache = ArtCache::default();
            let mut scene = Scene::new(ui, &data, &visual, &palette, &mut cache, width, height);
            scene.draw();
            scene.canvas.buffer
        };
        for theme in 0..THEMES.len() {
            for transparent in [false, true] {
                for (width, height, left, right, preferred) in [
                    (140, 40, true, true, Focus::Nav),
                    (80, 24, true, true, Focus::Nav),
                    (80, 24, true, true, Focus::Right),
                    (140, 40, false, false, Focus::Nav),
                    (140, 40, false, true, Focus::Right),
                    (140, 40, true, false, Focus::Nav),
                ] {
                    let mut app = App::default();
                    app.ui = Ui {
                        theme,
                        transparent,
                        plain_icons: transparent,
                        reduced_motion: true,
                        left_open: left,
                        right_open: right,
                        preferred,
                        ..Ui::default()
                    };
                    let layout = app.ui.fit(width.into(), height.into());
                    for key in ["tab", "tab", "tab", "backtab", "backtab", "backtab"] {
                        assert!(app.key(key, layout, 1.0, 1000.0));
                        let buffer = render(&app.ui, width, height);
                        let area = match app.ui.focus {
                            Focus::Nav => layout.nav.unwrap(),
                            Focus::Main => layout.main,
                            Focus::Right => layout.side.unwrap(),
                        };
                        let focus = Palette::new(theme, transparent)["focus"].color();
                        for x in 0..width {
                            let cell = &buffer[(x, 2)];
                            let active =
                                i32::from(x) > area.x && i32::from(x) < area.x + area.w - 1;
                            assert_eq!(cell.symbol() == "━", active);
                            if active {
                                assert_eq!(cell.fg, focus);
                                assert!(cell.modifier.contains(Modifier::BOLD));
                            }
                            if transparent {
                                assert_eq!(cell.bg, Color::Reset);
                            }
                        }
                        let title_x = area.x + if app.ui.focus == Focus::Main { 1 } else { 6 };
                        let title = &buffer[(title_x as u16, 1)];
                        assert_eq!(title.fg, focus);
                        assert!(title.modifier.contains(Modifier::BOLD));
                    }
                }
            }
        }
        for capture in 0..3 {
            let ui = Ui {
                help: capture == 0,
                dialog: (capture == 1).then_some(Dialog::Lyrics { cursor: 0 }),
                editor: (capture == 2).then(|| Editor::new(EditAction::Search, String::new())),
                ..Ui::default()
            };
            let buffer = render(&ui, 140, 40);
            assert!((0..140).all(|x| buffer[(x, 2)].symbol() != "━"));
        }
    }

    #[test]
    fn chrome_has_continuous_panels_and_glyph_controls() {
        let ui = Ui::default();
        let data = Data::default();
        let visual = Visual::settled(&ui, &data, 1000.0);
        let palette = Palette::new(0, false);
        let mut art_cache = ArtCache::default();
        let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut art_cache, 140, 40);
        assert!(scene.chrome());
        assert_eq!(
            scene.canvas.buffer[(0, 0)].bg,
            palette["panel.background"].color()
        );
        assert_eq!(
            scene.canvas.buffer[(139, 0)].bg,
            palette["panel.background"].color()
        );
        assert_eq!(
            scene.canvas.buffer[(3, 1)].symbol(),
            Icon::Headphones.glyph(false)
        );
        assert!(
            !scene
                .canvas
                .buffer
                .content
                .iter()
                .take(420)
                .any(|c| matches!(c.symbol(), "[" | "]"))
        );
        assert_eq!(
            scene
                .hits
                .iter()
                .filter(|h| matches!(h.action, Action::Go(_)))
                .count(),
            9
        );
    }
}
