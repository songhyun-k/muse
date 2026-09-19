use crate::{
    canvas::Canvas,
    geometry::{Area, ease},
    state::{Data, Panel, Ui, View, item_key},
    theme::Ink,
};
use ratatui::{buffer::Buffer, style::Color};

type Stamp = (View, Option<String>, usize, bool, Panel, bool, bool);
const SYMBOL_REVEAL: f64 = 0.35;

#[derive(Clone, Default)]
pub struct Effects {
    stamp: Option<Stamp>,
    last: Option<Buffer>,
    from: Option<Buffer>,
    since: f64,
}

impl Effects {
    /// A target stays inactive while any of its rows can show previous content.
    pub fn current_content_visible(&self, area: Area, now: f64) -> bool {
        self.from.as_ref().is_none_or(|old| {
            row_blend(
                (now - self.since) / 0.26,
                (area.y + area.h - 1).max(0) as usize,
                usize::from(old.area.height),
            ) >= SYMBOL_REVEAL
        })
    }

    pub fn active(&self, now: f64, reduced: bool) -> bool {
        !reduced && self.stamp.is_some() && now - self.since < 0.26
    }

    pub fn observe(&mut self, ui: &Ui, data: &Data, now: f64) {
        let stamp = (
            ui.view,
            data.detail.as_ref().map(|i| item_key(&i.r#ref)),
            ui.theme,
            ui.transparent,
            ui.panel,
            ui.help,
            ui.dialog.is_some(),
        );
        if ui.reduced_motion {
            self.from = None;
            self.since = now - 1.0;
        } else if self.stamp.as_ref() != Some(&stamp) {
            self.from = self.last.take().or(self.from.take());
            self.since = now;
        }
        self.stamp = Some(stamp);
    }

    pub fn apply(&mut self, ui: &Ui, data: &Data, now: f64, canvas: &mut Canvas<'_>) {
        self.observe(ui, data, now);
        if self
            .last
            .as_ref()
            .or(self.from.as_ref())
            .is_some_and(|old| old.area != canvas.buffer.area)
        {
            self.since = now - 1.0;
            self.from = None;
        }
        if self.active(now, ui.reduced_motion) {
            self.from.get_or_insert_with(|| {
                Canvas::new(
                    canvas.buffer.area.width,
                    canvas.buffer.area.height,
                    canvas.palette,
                )
                .buffer
            });
            reveal(
                &mut canvas.buffer,
                self.from.as_ref().unwrap(),
                (now - self.since) / 0.26,
                ui.transparent,
            );
        } else {
            self.from = None;
        }
        self.last = Some(canvas.buffer.clone());
    }
}

pub fn mix_color(before: Color, after: Color, amount: f64) -> Color {
    match (before, after) {
        (Color::Rgb(r, g, b), Color::Rgb(x, y, z)) => {
            Ink::Rgb(r, g, b).mix(Ink::Rgb(x, y, z), amount).color()
        }
        _ if amount <= 0.0 => before,
        _ => after,
    }
}

/// Switch complete rows together so a Korean/emoji grapheme is never split.
pub fn reveal(next: &mut Buffer, old: &Buffer, progress: f64, transparent: bool) {
    if next.area != old.area {
        return;
    }
    let width = usize::from(next.area.width.max(1));
    let height = usize::from(next.area.height);
    for (y, (row, previous)) in next
        .content
        .chunks_mut(width)
        .zip(old.content.chunks(width))
        .enumerate()
    {
        let blend = row_blend(progress, y, height);
        for (new, before) in row.iter_mut().zip(previous) {
            if new == before
                || (transparent && (new.bg == Color::Reset || before.bg == Color::Reset))
            {
                continue;
            }
            let bg = mix_color(before.bg, new.bg, blend);
            let fg = if new.symbol() == before.symbol() {
                mix_color(before.fg, new.fg, blend)
            } else if blend >= SYMBOL_REVEAL {
                mix_color(bg, new.fg, (blend - SYMBOL_REVEAL) / (1.0 - SYMBOL_REVEAL))
            } else {
                *new = before.clone();
                mix_color(bg, before.fg, 1.0 - blend / SYMBOL_REVEAL)
            };
            new.set_fg(fg).set_bg(bg);
        }
    }
}

fn row_blend(progress: f64, row: usize, height: usize) -> f64 {
    ease(progress * 1.18 - row as f64 / height.max(1) as f64 * 0.18)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    #[test]
    fn reveal_switches_complete_wide_text_and_never_invents_a_terminal_background() {
        let p = Palette::new(0, false);
        let mut before = Canvas::new(10, 2, &p);
        before.text(0, 0, "한글", p["text.primary"]);
        let mut after = Canvas::new(10, 2, &p);
        after.text(0, 0, "ABCD", p["accent"]);
        let expected = after.buffer.clone();
        reveal(&mut after.buffer, &before.buffer, 0.01, false);
        assert_eq!(after.buffer[(0, 0)].symbol(), "한");
        assert_eq!(after.buffer[(1, 0)].symbol(), "");
        reveal(&mut after.buffer, &expected, 0.0, false);
        assert_eq!(after.buffer[(0, 0)].symbol(), "A");
        let transparent = Palette::new(0, true);
        let mut canvas = Canvas::new(10, 2, &transparent);
        let unchanged = canvas.buffer.clone();
        reveal(&mut canvas.buffer, &before.buffer, 0.1, true);
        assert_eq!(canvas.buffer, unchanged);
        let mut effects = Effects::default();
        let mut ui = Ui::default();
        effects.apply(&ui, &Data::default(), 0.0, &mut before);
        assert!(effects.active(0.1, false));
        ui.reduced_motion = true;
        effects.apply(&ui, &Data::default(), 0.1, &mut before);
        assert!(!effects.active(0.1, true));
    }
}
