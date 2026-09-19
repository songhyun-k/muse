use crate::{
    canvas::{cells, clock},
    geometry::Area,
    icons::Icon,
    scene::{Action, Scene},
    state::View,
};

impl Scene<'_> {
    pub fn button(&mut self, x: i32, y: i32, text: &str, key: &'static str) {
        let p = self.canvas.palette;
        let icon = match key {
            "enter" | "play_detail" => Icon::Play,
            "s" | "shuffle_detail" => Icon::Shuffle,
            _ => Icon::Check,
        };
        let label = format!("{}  {text}", self.icon(icon));
        let area = Area::new(x, y, cells(&label) + 4, 1);
        let active = matches!(key, "s" | "shuffle_detail")
            && self.data.player.as_ref().is_some_and(|p| p.shuffle);
        let bg = p[if self.hovered(area) {
            "control.hover"
        } else if active {
            "control.active"
        } else {
            "control.background"
        }];
        self.capsule(area, bg, p["accent"], &format!(" {label}"), true);
        self.hit(area, Action::Key(key));
    }

    pub fn cards_view(&mut self) {
        let area = self.heading();
        self.cards(area);
    }

    pub fn cards(&mut self, area: Area) {
        let Area { x, y, w, h } = area;
        let data = self.data;
        let rows = &data.items;
        let cursor = self.ui.cursors[0].min(rows.len().saturating_sub(1));
        let p = self.canvas.palette;
        if h < 14 || w < 46 {
            let capacity = (h / 4).max(1) as usize;
            let start = cursor.saturating_add(1).saturating_sub(capacity);
            for (row, (i, item)) in rows
                .iter()
                .enumerate()
                .skip(start)
                .take(capacity)
                .enumerate()
            {
                let yy = y + row as i32 * 4;
                self.art(Area::new(x, yy, 6, 3), item);
                let title = if self.ui.view == View::Artists && !item.album.is_empty() {
                    &item.album
                } else {
                    &item.title
                };
                self.canvas.label(
                    x + 9,
                    yy,
                    title,
                    p[if i == cursor {
                        "accent"
                    } else {
                        "text.primary"
                    }],
                    false,
                    w - 9,
                );
                self.canvas.label(
                    x + 9,
                    yy + 2,
                    &item.artist,
                    p["text.secondary"],
                    false,
                    w - 9,
                );
                self.hit(Area::new(x, yy, w, 3), Action::Open(item.r#ref.clone()));
            }
            return;
        }
        let cols = ((w + 3) / 21).clamp(1, 5);
        let card_w = ((w - (cols - 1) * 3) / cols).min(20);
        let art_h = (card_w / 2).min((h - 4).max(4));
        let visible_rows = ((h + 2) / (art_h + 5)).max(1) as usize;
        let start = (cursor / cols as usize)
            .saturating_add(1)
            .saturating_sub(visible_rows)
            * cols as usize;
        for (i, item) in rows.iter().enumerate().skip(start) {
            let yy = y + (i - start) as i32 / cols * (art_h + 5);
            if yy + art_h + 3 > y + h {
                break;
            }
            let xx = x + (i as i32 % cols) * (card_w + 3);
            let area = Area::new(xx, yy, card_w, art_h + 3);
            self.art(Area::new(xx, yy, card_w, art_h), item);
            if self.hovered(area) {
                self.canvas.rule(xx, yy + art_h, card_w, p["fx.glow"]);
            }
            self.canvas.label(
                xx,
                yy + art_h + 1,
                &item.title,
                p[if i == cursor {
                    "accent"
                } else {
                    "text.primary"
                }],
                i == cursor,
                card_w,
            );
            self.canvas.label(
                xx,
                yy + art_h + 2,
                if item.r#ref.kind == crate::generated::Kind::Artist {
                    self.ui.text("앨범 보기")
                } else {
                    &item.artist
                },
                p["text.secondary"],
                false,
                card_w,
            );
            self.hit(area, Action::Open(item.r#ref.clone()));
        }
    }

    pub fn detail_view(&mut self) {
        if self
            .data
            .detail
            .as_ref()
            .is_some_and(|i| i.r#ref.kind == crate::generated::Kind::Artist)
        {
            self.cards_view();
            return;
        }
        let Area { x, y, w, h } = self.heading();
        let Some(item) = &self.data.detail else {
            return;
        };
        let p = self.canvas.palette;
        let hero_h = if h >= 19 && w >= 60 { 6 } else { 4 };
        let hero_w = hero_h * 2;
        self.art(Area::new(x, y, hero_w, hero_h), item);
        let xx = x + hero_w + 3;
        let ww = w - hero_w - 3;
        let artist = if self.ui.view == View::Playlists {
            self.ui.text("플레이리스트")
        } else {
            &item.artist
        };
        self.canvas
            .label(xx, y, &item.title, p["text.primary"], true, ww);
        self.canvas.label(xx, y + 1, artist, p["accent"], false, ww);
        let duration = self.data.items.iter().filter_map(|i| i.duration).sum();
        let more = if self.data.next_offset.is_some() {
            "+"
        } else {
            ""
        };
        self.canvas.label(
            xx,
            y + 2,
            &format!(
                "{}{more}{} · {}{more}",
                self.data.items.len(),
                self.ui.language.unit(self.data.items.len(), true),
                clock(duration)
            ),
            p["text.secondary"],
            false,
            ww,
        );
        if hero_h >= 6
            && let Some(collection) = self.data.store.as_ref().and_then(|s| {
                s.collections.iter().find(|c| {
                    c.id == item.r#ref.id
                        && item.r#ref.source == crate::generated::Source::Collection
                })
            })
        {
            self.canvas.label(
                xx,
                y + 3,
                &collection.description,
                p["text.secondary"],
                false,
                ww,
            );
        }
        if hero_h >= 6 {
            self.button(xx, y + 4, self.ui.text("재생"), "play_detail");
            if ww >= 26 {
                self.button(xx + 13, y + 4, self.ui.text("셔플"), "shuffle_detail");
            }
        }
        self.tracklist(Area::new(x, y + hero_h + 1, w, h - hero_h - 1), false, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        art::ArtCache,
        generated::{Notice, Page},
        geometry::Visual,
        requests::{Response, Target},
        state::{Data, Ui},
        theme::Palette,
    };

    #[test]
    fn card_pagination_keeps_the_selected_card_visible() {
        let mut data = Data::default();
        data.apply(Response {
            target: Some(Target::Main),
            sequence: 1,
            notice: Notice::Page(Page {
                items: (0..30)
                    .map(|i| {
                        serde_json::from_value(serde_json::json!({
                            "ref":{"source":"library","kind":"album","id":i.to_string()},
                            "title":format!("앨범 {i}"),"artist":"유나","album":format!("앨범 {i}")
                        }))
                        .unwrap()
                    })
                    .collect(),
                next_offset: None,
            }),
        });
        let ui = Ui {
            view: View::Albums,
            cursors: [29, 0],
            ..Ui::default()
        };
        let visual = Visual::settled(&ui, &data, 0.0);
        let palette = Palette::new(0, false);
        let mut cache = ArtCache::default();
        for (w, h) in [(80, 24), (140, 40), (180, 44)] {
            let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut cache, w, h);
            scene.cards_view();
            assert!(
                scene
                    .hits
                    .iter()
                    .any(|h| matches!(&h.action, Action::Open(item) if item.id == "29"))
            );
        }
        data.detail = Some(
            serde_json::from_value(serde_json::json!({
                "ref":{"source":"catalog","kind":"artist","id":"artist"},
                "title":"유나","artist":"유나","album":""
            }))
            .unwrap(),
        );
        let ui = Ui {
            view: View::Detail,
            ..Ui::default()
        };
        let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut cache, 140, 40);
        scene.draw();
        assert!(
            scene
                .hits
                .iter()
                .any(|h| matches!(h.action, Action::Open(_)))
        );
        assert!(
            !scene
                .hits
                .iter()
                .any(|h| matches!(h.action, Action::Select { queue: false, .. }))
        );
    }
}
