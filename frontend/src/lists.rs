use crate::{
    canvas::{cells, clock},
    generated::Kind,
    geometry::Area,
    icons::Icon,
    scene::{Action, Scene},
    state::{Focus, View},
};

impl Scene<'_> {
    /// Decorative movement only; no audio spectrum is measured or implied.
    pub fn spectrum(&mut self, x: i32, y: i32, width: i32) {
        let p = self.canvas.palette;
        let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for i in 0..width {
            let index = f64::from(i);
            let envelope = (0.48 + 0.35 * (index * 0.83 + self.visual.art_time * 2.8).sin())
                * (0.75 + 0.25 * (index * 0.3 - self.visual.art_time).sin());
            let value = (envelope * self.visual.energy).clamp(0.0, 1.0);
            self.canvas.text(
                x + i,
                y,
                &bars[(value * 7.0).round_ties_even() as usize].to_string(),
                p["wave.low"].mix(p["wave.high"], value),
            );
        }
    }

    pub fn heading(&mut self) -> Area {
        let Area { x, y, w, h } = self.layout.main;
        let p = self.canvas.palette;
        let heading = match self.ui.view {
            View::Detail
                if self
                    .data
                    .detail
                    .as_ref()
                    .is_some_and(|i| i.r#ref.kind == Kind::Artist) =>
            {
                View::Artists
            }
            View::Detail
                if self
                    .data
                    .detail
                    .as_ref()
                    .is_some_and(|i| i.r#ref.kind == Kind::Playlist) =>
            {
                View::Playlists
            }
            View::Detail | View::Lyrics => View::Albums,
            view => view,
        };
        let icon = match heading {
            View::Albums => Icon::Album,
            View::Playlists => Icon::Playlist,
            View::Search => Icon::Search,
            View::Artists => Icon::Artist,
            View::Favorites => Icon::Heart,
            View::History => Icon::History,
            _ => Icon::Music,
        };
        self.canvas.text(x, y, self.icon(icon), p["accent"]);
        let title = self
            .data
            .detail
            .as_ref()
            .filter(|i| self.ui.view == View::Detail && i.r#ref.kind == Kind::Artist)
            .map_or(self.ui.text(heading.label()), |i| i.title.as_str());
        self.canvas
            .label(x + 3, y, title, p["text.primary"], true, w - 12);
        let entities = self.ui.view.cards()
            || (self.ui.view == View::Search && self.ui.search_kind != Kind::Song)
            || (self.ui.view == View::Playlists && self.data.detail.is_none())
            || self
                .data
                .detail
                .as_ref()
                .is_some_and(|i| i.r#ref.kind == Kind::Artist);
        let more = if self.data.next_offset.is_some() && self.ui.view != View::Lyrics {
            "+"
        } else {
            ""
        };
        let count = if self.data.loading.contains(&crate::requests::Target::Main) {
            "  …".into()
        } else {
            let count = if self.ui.view == View::Lyrics {
                usize::from(self.data.current().is_some())
            } else {
                self.data.items.len()
            };
            let unit = self
                .ui
                .language
                .unit(count, !entities || self.ui.view == View::Lyrics);
            format!(" {count:02}{unit}{more}")
        };
        let badge_w = (cells(&count) + 2).max(8);
        self.capsule(
            Area::new(x + w - badge_w, y, badge_w, 1),
            p["badge.background"],
            p["badge.text"],
            &count,
            false,
        );
        Area::new(x, y + 2, w, h - 2)
    }

    pub fn song_view(&mut self) {
        let mut area = self.heading();
        if self.ui.view == View::Search {
            let p = self.canvas.palette;
            self.canvas
                .fill(area.x, area.y, area.w, 1, p["control.background"]);
            let value = self
                .ui
                .editor
                .as_ref()
                .filter(|e| matches!(e.action, crate::state::EditAction::Search))
                .map_or(&self.ui.query, |e| &e.buffer);
            let placeholder = if value.is_empty() {
                self.ui.text("아티스트, 노래 또는 앨범")
            } else {
                value
            };
            let caret = if self.ui.editor.is_some() { "▏" } else { "" };
            let text = if let Some(editor) = self.ui.editor.as_ref().filter(|e| {
                matches!(e.action, crate::state::EditAction::Search) && !e.buffer.is_empty()
            }) {
                format!("/ {}", editor.visible(area.w - 4))
            } else {
                format!("/ {placeholder}{caret}")
            };
            self.canvas.label(
                area.x + 1,
                area.y,
                &text,
                p[if value.is_empty() {
                    "text.secondary"
                } else {
                    "text.primary"
                }],
                false,
                area.w - 2,
            );
            self.hit(Area::new(area.x, area.y, area.w, 1), Action::Key("/"));
            if self.ui.search_kind != Kind::Song
                || self.ui.search_source == crate::generated::Source::Library
            {
                let kind = match self.ui.search_kind {
                    Kind::Song => self.ui.text("노래"),
                    Kind::Album => self.ui.text("앨범"),
                    Kind::Artist => self.ui.text("아티스트"),
                    Kind::Playlist => self.ui.text("플레이리스트"),
                    Kind::Station => self.ui.text("스테이션"),
                };
                let source = if self.ui.search_source == crate::generated::Source::Library {
                    self.ui.text("보관함")
                } else {
                    "Apple Music"
                };
                self.canvas.label(
                    area.x + 1,
                    area.y + 1,
                    &format!("{source} · {kind}"),
                    p["text.quiet"],
                    false,
                    area.w - 2,
                );
            }
            area.y += 2;
            area.h = (area.h - 2).max(0);
            if self.ui.search_kind != Kind::Song {
                self.cards(area);
                if self.data.items.is_empty() {
                    self.canvas.label(
                        area.x + 1,
                        area.y + 1,
                        self.ui.text("검색 결과가 없습니다"),
                        p["text.secondary"],
                        false,
                        area.w - 2,
                    );
                }
                return;
            }
        }
        self.tracklist(area, false, self.canvas.height() < 38);
    }

    pub fn tracklist(&mut self, area: Area, queue: bool, compact: bool) {
        let Area { x, mut y, w, mut h } = area;
        if h < 1 {
            return;
        }
        let data = self.data;
        let queue_rows = data
            .queue
            .as_ref()
            .map(|q| q.entries.as_slice())
            .unwrap_or_default();
        let len = if queue {
            queue_rows.len()
        } else {
            data.items.len()
        };
        let p = self.canvas.palette;
        if len == 0 {
            self.canvas.label(
                x + 1,
                y + (h - 1).min(1),
                &format!(
                    "{}  {}",
                    self.icon(Icon::Music),
                    self.ui.text("표시할 곡이 없습니다")
                ),
                p["text.secondary"],
                false,
                w - 2,
            );
            return;
        }
        let step = if compact { 1 } else { 2 };
        let rich = !compact && !queue && w >= 48;
        let artist_column = compact && w >= 62;
        let album_column = !queue && w >= if compact { 92 } else { 66 };
        let title_x = x + if rich { 8 } else { 5 };
        let artist_x = x + w - if album_column { 42 } else { 24 };
        let album_x = x + w - 26;
        let title_end = if artist_column {
            artist_x
        } else if album_column {
            album_x
        } else {
            x + w - 9
        };
        let title_w = if queue {
            (w - 8).max(5)
        } else {
            (title_end - title_x - 2).max(5)
        };
        if !queue && h >= 4 {
            self.canvas
                .text(title_x, y, self.ui.text("노래"), p["text.quiet"]);
            if artist_column {
                self.canvas
                    .text(artist_x, y, self.ui.text("아티스트"), p["text.quiet"]);
            }
            if album_column {
                self.canvas
                    .text(album_x, y, self.ui.text("앨범"), p["text.quiet"]);
            }
            self.canvas
                .text(x + w - 8, y, self.ui.text("시간"), p["text.quiet"]);
            y += 2;
            h -= 2;
        }
        let capacity = (h / step).max(1) as usize;
        let pane = usize::from(queue);
        let cursor = self.ui.cursors[pane].min(len - 1);
        let start = cursor
            .saturating_add(1)
            .saturating_sub(capacity)
            .min(len.saturating_sub(capacity));
        let focus = if queue {
            self.ui.queue_focus()
        } else {
            self.ui.focus == Focus::Main
        };
        for (line, index) in (start..len.min(start + capacity)).enumerate() {
            let yy = y + line as i32 * step;
            let item = if queue {
                queue_rows[index].item.as_ref()
            } else {
                Some(&data.items[index])
            };
            let row = Area::new(x, yy, w, step);
            let hover = self.hovered(row);
            let strength = if focus {
                (-(index as f64 - self.visual.selections[pane]).powi(2) * 3.0).exp()
            } else {
                0.0
            };
            if strength > 0.01 || hover {
                for col in 0..w {
                    let amount =
                        strength * (0.95 - 0.35 * f64::from(col) / f64::from((w - 1).max(1)));
                    let color = if hover && strength < 0.08 {
                        p["control.hover"]
                    } else {
                        p["background"].mix(p["selection.background"], amount)
                    };
                    self.canvas
                        .fill(x + col, yy, 1, step.min(h - line as i32 * step), color);
                }
            }
            if strength > 0.06 {
                let edge = p[if self.ui.transparent {
                    "fx.glow"
                } else {
                    "background"
                }]
                .mix(p["selection.edge"], strength);
                for dy in 0..step {
                    self.canvas.text(x, yy + dy, "▎", edge);
                }
            }
            let current =
                !queue && item.is_some_and(|i| data.current().is_some_and(|c| c.r#ref == i.r#ref));
            if current {
                if data.player.as_ref().is_some_and(|p| p.playing) || self.visual.energy > 0.05 {
                    self.spectrum(x + 1, yy, 2);
                } else {
                    self.canvas
                        .text(x + 1, yy, self.icon(Icon::Pause), p["accent"]);
                }
            } else {
                self.canvas
                    .text(x + 1, yy, &format!("{:02}", index + 1), p["text.quiet"]);
            }
            if rich && let Some(item) = item {
                self.art(Area::new(x + 4, yy, 2, 1), item);
            }
            let title = item.map_or(self.ui.text("정보 불러오는 중"), |i| {
                i.title.as_str()
            });
            let artist = item.map_or("", |i| i.artist.as_str());
            self.canvas.label(
                title_x,
                yy,
                title,
                p[if focus && index == cursor {
                    "selection.text"
                } else {
                    "text.primary"
                }],
                focus && index == cursor,
                title_w,
            );
            if !compact {
                self.canvas.label(
                    title_x,
                    yy + 1,
                    artist,
                    p["text.secondary"],
                    false,
                    if queue { (w - 13).max(3) } else { title_w },
                );
            }
            if artist_column {
                self.canvas
                    .label(artist_x, yy, artist, p["text.secondary"], false, 15);
            }
            if album_column {
                self.canvas.label(
                    album_x,
                    yy,
                    item.map_or("", |i| i.album.as_str()),
                    p["text.secondary"],
                    false,
                    17,
                );
            }
            let duration = item
                .and_then(|i| i.duration)
                .map_or_else(|| "--:--".into(), clock);
            self.canvas.text(
                x + w - if queue { 6 } else { 8 },
                yy + i32::from(queue),
                &duration,
                p["text.secondary"],
            );
            if let Some(key) = data.row_key(index, queue) {
                self.hit(row, Action::Select { index, queue, key });
            }
            if let Some(item) = item.filter(|i| i.r#ref.kind == Kind::Song) {
                let favorite = data.favorite(item);
                if favorite || hover || (focus && index == cursor) {
                    let mut color = p[if favorite { "accent" } else { "text.quiet" }];
                    if !self.ui.reduced_motion
                        && let Some((reference, since)) = &self.ui.favorite_pulse
                        && *reference == item.r#ref
                        && self.visual.now - since < 0.5
                    {
                        color = p["fx.highlight"].mix(
                            p["accent"],
                            crate::geometry::ease((self.visual.now - since) / 0.5),
                        );
                    }
                    self.canvas.text(
                        x + w - 2,
                        yy,
                        self.icon(if favorite {
                            Icon::Heart
                        } else {
                            Icon::HeartEmpty
                        }),
                        color,
                    );
                }
                self.hit(
                    Area::new(x + w - 3, yy, 3, 1),
                    Action::Favorite(item.r#ref.clone()),
                );
            }
        }
        if len > capacity {
            let offset =
                (f64::from(h - 1) * cursor as f64 / (len - 1) as f64).round_ties_even() as i32;
            self.canvas.text(x + w - 1, y + offset, "▏", p["border"]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        art::ArtCache,
        generated::{Notice, Page, QueueEntry, QueuePage},
        geometry::Visual,
        requests::{Response, Target},
        state::{Data, Ui},
        theme::Palette,
    };

    #[test]
    fn lists_scroll_to_selection_and_retain_unresolved_queue_entries() {
        let mut data = Data::default();
        let items = (0..20)
            .map(|i| {
                serde_json::from_value(serde_json::json!({
                "ref":{"source":"library","kind":"song","id":i.to_string()},
                "title":format!("선택 곡 {i}"),"artist":"유나","album":"낮의 기록","duration":213
            })).unwrap()
            })
            .collect();
        data.apply(Response {
            target: Some(Target::Main),
            sequence: 1,
            notice: Notice::Page(Page {
                items,
                next_offset: None,
            }),
        });
        let ui = Ui {
            cursors: [19, 0],
            ..Ui::default()
        };
        let visual = Visual::settled(&ui, &data, 1000.0);
        let palette = Palette::new(0, false);
        let mut cache = ArtCache::default();
        let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut cache, 140, 40);
        scene.chrome();
        scene.song_view();
        let last = scene
            .hits
            .iter()
            .find(|hit| {
                matches!(
                    hit.action,
                    Action::Select {
                        index: 19,
                        queue: false,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(last.area.y + last.area.h <= 35);
        assert_eq!(
            scene.canvas.buffer[(last.area.x as u16, last.area.y as u16)].symbol(),
            "▎"
        );
        data.queue = Some(QueuePage {
            entries: vec![QueueEntry {
                id: "pending".into(),
                item: None,
            }],
            next_offset: None,
            revision: 1,
            total: 1,
        });
        let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut cache, 140, 40);
        scene.tracklist(Area::new(113, 6, 25, 27), true, false);
        assert!(scene.hits.iter().any(|hit| matches!(
            hit.action,
            Action::Select {
                index: 0,
                queue: true,
                ..
            }
        )));
        assert_eq!(scene.canvas.buffer[(118, 6)].symbol(), "정");
    }
}
