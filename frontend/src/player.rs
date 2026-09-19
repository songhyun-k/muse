use crate::{
    canvas::{cells, clock},
    generated::RepeatMode,
    geometry::Area,
    icons::Icon,
    scene::{Action, Scene},
    state::Panel,
};

impl Scene<'_> {
    pub fn player(&mut self) {
        let Area {
            y, w: outer_w, h, ..
        } = self.layout.player;
        let p = self.canvas.palette;
        let data = self.data;
        let player = data.player.as_ref();
        let playing = player.is_some_and(|p| p.playing);
        self.canvas.fill(0, y, outer_w, h, p["player.background"]);
        self.canvas.rule(1, y, self.canvas.width() - 2, p["border"]);
        let w = (outer_w - 6).min(126);
        let x = (self.canvas.width() - w) / 2;
        if let Some(item) = data.current() {
            self.blended_art(Area::new(x, y + 1, 4, 2), item);
        }
        let left_w = (w / 3 - 8).max(12);
        self.canvas.label(
            x + 6,
            y + 1,
            data.current()
                .map_or(self.ui.text("재생할 곡을 선택해주세요"), |i| {
                    &i.title
                }),
            p["player.background"].mix(p["player.text"], self.visual.cover_blend),
            true,
            left_w,
        );
        self.canvas.label(
            x + 6,
            y + 2,
            data.current().map_or("", |i| &i.artist),
            p["text.secondary"],
            false,
            left_w,
        );
        let center = x + w / 2 - 1;
        for (dx, icon, key, active) in [
            (-11, Icon::Shuffle, "s", player.is_some_and(|p| p.shuffle)),
            (-6, Icon::Previous, "b", false),
            (
                0,
                if playing { Icon::Pause } else { Icon::Play },
                " ",
                data.current().is_some(),
            ),
            (6, Icon::Next, "n", false),
            (
                11,
                Icon::Repeat,
                "r",
                player.is_some_and(|p| p.repeat_mode != RepeatMode::Off),
            ),
        ] {
            self.icon_button(center + dx, y + 1, icon, key, active);
        }
        let duration = data.current().and_then(|i| i.duration);
        let time = duration.map_or_else(|| "--:--".into(), clock);
        self.canvas.text(
            center - 6,
            y + 2,
            &format!("{} / {time}", clock(self.visual.position)),
            p["text.secondary"],
        );
        self.icon_button(
            x + w - 21,
            y + 1,
            Icon::Lyrics,
            "l",
            self.ui.right_open && self.ui.panel == Panel::Lyrics,
        );
        self.icon_button(
            x + w - 16,
            y + 1,
            Icon::Playlist,
            "Q",
            self.ui.right_open && self.ui.panel == Panel::Queue,
        );
        let volume = data.volume.as_ref();
        let icon = if volume.is_some_and(|v| v.muted == Some(true)) {
            Icon::Mute
        } else {
            Icon::Volume
        };
        if volume.is_some_and(|v| v.can_mute) {
            self.icon_button(x + w - 11, y + 1, icon, "m", false);
        } else {
            self.canvas
                .text(x + w - 11, y + 1, self.icon(icon), p["text.quiet"]);
        }
        let value = if volume.and_then(|v| v.level).is_some() {
            format!("{:3}%", self.visual.volume.round_ties_even() as i32)
        } else {
            "   —".into()
        };
        self.canvas
            .text(x + w - 7, y + 1, &value, p["text.secondary"]);
        for i in 0..8 {
            self.canvas.text(
                x + w - 10 + i,
                y + 2,
                "━",
                p[if f64::from(i) / 8.0 < self.visual.volume / 100.0 {
                    "accent"
                } else {
                    "progress.track"
                }],
            );
        }
        if volume.is_some_and(|v| v.can_set_volume) {
            self.hit(Area::new(x + w - 10, y + 2, 8, 1), Action::Volume);
        }
        self.progress(Area::new(x, y + 3, w, 1), duration);
        if player.is_some_and(|p| p.can_seek) {
            self.hit(Area::new(x, y + 3, w, 1), Action::Seek);
        }
        let hints = format!(
            "{}  {}",
            self.icon(Icon::Keyboard),
            self.ui.text("Tab 탐색    / 검색    Space 재생")
        );
        let tail = format!(
            "{} {}",
            self.icon(Icon::Help),
            self.ui.text("? 도움말  q 종료")
        );
        let settings = format!("{} , {}", self.icon(Icon::Settings), self.ui.text("설정"));
        let settings_area = Area::new(
            x + w - cells(&tail) - cells(&settings) - 3,
            y + 4,
            cells(&settings),
            1,
        );
        self.canvas.label(
            x,
            y + 4,
            &hints,
            p["text.quiet"],
            false,
            settings_area.x - x - 3,
        );
        self.canvas.text(
            settings_area.x,
            settings_area.y,
            &settings,
            p[if self.hovered(settings_area) {
                "accent"
            } else {
                "text.secondary"
            }],
        );
        self.hit(settings_area, Action::Key(","));
        self.canvas
            .text(x + w - cells(&tail), y + 4, &tail, p["text.quiet"]);
    }

    fn progress(&mut self, area: Area, duration: Option<f64>) {
        let p = self.canvas.palette;
        let progress = duration
            .filter(|n| *n > 0.0)
            .map_or(0.0, |n| self.visual.position / n * f64::from(area.w));
        let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for i in 0..area.w {
            let index = f64::from(i);
            let envelope = 0.25
                + 0.45
                    * ((index * 0.42 + self.visual.track_phase).sin() * (index * 0.19).cos()).abs();
            let live = (-((index - progress) / 5.0).powi(2)).exp()
                * self.visual.energy
                * 0.2
                * (self.visual.art_time * 4.0 + index).sin();
            let glyph = bars[((envelope + live).clamp(0.0, 1.0) * 7.0).round_ties_even() as usize];
            let played = p["progress.fill"].mix(
                p["accent.secondary"],
                index / f64::from((area.w - 1).max(1)),
            );
            let color = if (index - progress).abs() < 1.0 {
                p["progress.head"]
            } else if index < progress {
                played
            } else {
                p["progress.track"]
            };
            self.canvas
                .text(area.x + i, area.y, &glyph.to_string(), color);
        }
    }

    pub fn queue_panel(&mut self) {
        let Some(Area { x, y, w, h }) = self.layout.side else {
            return;
        };
        let count = self.data.player.as_ref().map_or(0, |p| p.queue_count);
        self.canvas.label(
            x + 3,
            y,
            &if self.ui.language == crate::i18n::Language::Korean {
                format!("다음에 들을 {count:02}곡")
            } else {
                format!("Up next · {count:02}")
            },
            self.canvas.palette["text.secondary"],
            false,
            w - 6,
        );
        let target = crate::requests::Target::Queue;
        let failed = self.data.errors.contains_key(&target);
        if failed || (self.data.loading.contains(&target) && self.data.queue.is_none()) {
            self.canvas.label(
                x + 3,
                y + 2,
                if failed {
                    self.ui.text("큐를 불러오지 못했어요")
                } else {
                    self.ui.text("불러오는 중")
                },
                self.canvas.palette["text.secondary"],
                false,
                w - 6,
            );
            if failed {
                self.canvas.label(
                    x + 3,
                    y + 4,
                    self.ui.text("Enter 다시 시도"),
                    self.canvas.palette["accent"],
                    false,
                    w - 6,
                );
                self.hit(
                    Area::new(x + 3, y + 4, w - 6, 1),
                    Action::Key("retry_queue"),
                );
            }
        } else {
            self.tracklist(Area::new(x + 2, y + 2, w - 4, h - 3), true, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        art::ArtCache,
        geometry::Visual,
        state::{Data, Ui},
        theme::Palette,
    };
    #[test]
    fn unavailable_seek_and_hardware_controls_have_no_mouse_actions() {
        let ui = Ui::default();
        let data = Data::default();
        let visual = Visual::settled(&ui, &data, 0.0);
        let palette = Palette::new(0, true);
        let mut cache = ArtCache::default();
        let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut cache, 140, 40);
        scene.player();
        assert!(
            !scene
                .hits
                .iter()
                .any(|hit| matches!(hit.action, Action::Seek | Action::Volume | Action::Key("m")))
        );
        assert!(
            scene
                .canvas
                .buffer
                .content
                .iter()
                .all(|c| c.bg == ratatui::style::Color::Reset)
        );
    }
}
