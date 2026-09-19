use crate::{
    canvas::cells,
    geometry::Area,
    icons::Icon,
    requests::Target,
    scene::{Action, Scene},
    state::Dialog,
};

impl Scene<'_> {
    pub fn dialog(&mut self) {
        let Some(dialog) = &self.ui.dialog else {
            return;
        };
        let rows = dialog.rows(self.ui, self.data);
        let lyrics = matches!(dialog, Dialog::Lyrics { .. });
        let settings = matches!(dialog, Dialog::Settings { .. });
        let title = match dialog {
            Dialog::Menu { title, .. } => title.as_str(),
            Dialog::Lyrics { .. } => self.ui.text("가사 선택"),
            Dialog::Settings { .. } => self.ui.text("설정"),
        };
        let step = if lyrics || settings { 2 } else { 1 };
        let capacity = ((self.canvas.height() - 8) / step).clamp(1, 12) as usize;
        let shown = rows.len().min(capacity);
        let w = 64.min(self.canvas.width() - 8);
        let h = (shown as i32 * step + 5 + i32::from(settings))
            .max(9)
            .min(self.canvas.height() - if settings { 2 } else { 4 });
        let x = (self.canvas.width() - w) / 2;
        let y = (self.canvas.height() - h) / 2;
        let p = self.canvas.palette;
        let secondary = p["overlay.text"].mix(p["overlay.background"], 0.45);
        self.hits.clear();
        self.canvas.fill(x, y, w, h, p["overlay.background"]);
        self.canvas.rule(x, y, w, p["overlay.accent"]);
        self.canvas
            .label(x + 3, y + 1, title, p["overlay.accent"], true, w - 6);
        if settings {
            self.canvas.label(
                x + 3,
                y + 2,
                self.ui.text("변경 사항이 즉시 적용됩니다"),
                secondary,
                false,
                w - 6,
            );
            self.canvas.text(x + w - 4, y + 1, "×", secondary);
            self.hit(Area::new(x + w - 5, y + 1, 3, 1), Action::Key("esc"));
        }
        let cursor = dialog.cursor().min(rows.len().saturating_sub(1));
        let start = cursor
            .saturating_add(1)
            .saturating_sub(capacity)
            .min(rows.len().saturating_sub(capacity));
        if rows.is_empty() {
            let message = if let Some(error) = self.data.errors.get(&Target::LyricMatches) {
                error.message.as_str()
            } else if self.data.loading.contains(&Target::LyricMatches) {
                self.ui.text("불러오는 중")
            } else {
                self.ui.text("검색 결과가 없습니다")
            };
            self.canvas.label(
                x + 3,
                y + 3,
                &self.ui.message(message),
                secondary,
                false,
                w - 6,
            );
            if lyrics && !self.data.loading.contains(&Target::LyricMatches) {
                self.canvas.text(
                    x + 3,
                    y + 5,
                    self.ui.text("M 다시 검색"),
                    p["overlay.accent"],
                );
                self.hit(Area::new(x + 3, y + 5, w - 6, 1), Action::Key("M"));
            }
        }
        for (line, (index, row)) in rows
            .iter()
            .enumerate()
            .skip(start)
            .take(capacity)
            .enumerate()
        {
            let yy = y + 3 + i32::from(settings) + line as i32 * step;
            let area = Area::new(x + 1, yy, w - 2, step);
            let hover = self.ui.hover.is_some_and(|point| area.contains(point));
            if index == cursor || hover {
                let bg = p[if hover {
                    "control.hover"
                } else {
                    "selection.background"
                }];
                let bg = if matches!(bg, crate::theme::Ink::Rgb(..)) {
                    bg
                } else {
                    p["overlay.background"].mix(p["overlay.accent"], 0.12)
                };
                self.canvas.fill(area.x, area.y, area.w, area.h, bg);
                self.canvas.text(x + 1, yy, "▎", p["selection.edge"]);
            }
            self.canvas.label(
                x + 3,
                yy,
                &row.label,
                p["overlay.text"],
                index == cursor,
                if settings { w - 30 } else { w - 8 },
            );
            if settings {
                let value = format!("{}  {}", row.detail, self.icon(Icon::Chevron));
                let value_width = cells(&value).min(w / 2);
                self.canvas.label(
                    x + w - 3 - value_width,
                    yy,
                    &value,
                    p["overlay.accent"],
                    index == cursor,
                    value_width,
                );
            } else if step == 2 {
                self.canvas
                    .label(x + 3, yy + 1, &row.detail, secondary, false, w - 6);
            }
            if row.checked {
                self.canvas
                    .text(x + w - 4, yy, self.icon(Icon::Check), p["overlay.accent"]);
            }
            self.hit(area, Action::Dialog(index));
        }
        self.canvas.label(
            x + 3,
            y + h - 2,
            self.ui.text(if settings {
                "↑↓ 선택  ←→ 변경  Esc 닫기"
            } else {
                "↑↓ 선택   Enter 확인   Esc 닫기"
            }),
            secondary,
            false,
            w - 6,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::App,
        art::ArtCache,
        geometry::Visual,
        state::{Choice, MenuRow},
        theme::Palette,
    };
    #[test]
    fn modal_keeps_last_option_visible_and_blocks_background_mouse_targets() {
        let mut app = App::default();
        app.ui.dialog = Some(Dialog::Menu {
            title: "목록".into(),
            cursor: 99,
            rows: (0..100)
                .map(|i| MenuRow {
                    label: format!("목록 {i}"),
                    detail: String::new(),
                    checked: false,
                    choice: Choice::Close,
                })
                .collect(),
        });
        let p = Palette::new(3, true);
        let visual = Visual::settled(&app.ui, &app.data, 0.0);
        let mut cache = ArtCache::default();
        let mut scene = Scene::new(&app.ui, &app.data, &visual, &p, &mut cache, 80, 24);
        scene.draw();
        assert!(
            scene
                .hits
                .iter()
                .any(|h| matches!(h.action, Action::Dialog(99)))
        );
        assert!(
            scene
                .hits
                .iter()
                .all(|h| matches!(h.action, Action::Dialog(_)))
        );
        assert!(scene.hits.iter().all(|h| h.area.y + h.area.h <= 24));
        let selected = scene
            .hits
            .iter()
            .find(|h| matches!(h.action, Action::Dialog(99)))
            .unwrap();
        assert_ne!(
            scene.canvas.buffer[((selected.area.x + 1) as u16, selected.area.y as u16)].bg,
            ratatui::style::Color::Reset
        );
    }
}
